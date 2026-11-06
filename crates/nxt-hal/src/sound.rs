//! Tone output through the SSC, ported from NxOS `base/drivers/sound.c`:
//! a 1-bit pulse-density sine streamed by the SSC's DMA (PDC), chained one
//! cycle at a time and polled (no interrupts). Requires
//! `Pmc::init_clocks` to have run: the divider assumes MCK ~= 48.05 MHz.

use nxt_pac::pio::{ClockEnabled as PioClock, Pio};
use nxt_pac::ssc::{self, ClockEnabled as SscClock, Ssc};

const PA17_TD: u32 = 1 << 17;

/// One period of a 1-bit pulse-density sine wave.
const TONE_PATTERN: [u32; 16] = [
    0xF0F0F0F0, 0xF0F0F0F0, 0xFCFCFCFC, 0xFCFCFCFC, 0xFFFFFFFF, 0xFFFFFFFF, 0xFCFCFCFC, 0xFCFCFCFC,
    0xF0F0F0F0, 0xF0F0F0F0, 0xC0C0C0C0, 0xC0C08080, 0x00000000, 0x00000000, 0x8080C0C0, 0xC0C0C0C0,
];

/// Periods per DMA buffer. The refill must land before the current buffer
/// ends; at one period per buffer that is 0.5 ms at 2 kHz, which an AVR
/// transaction plus a USB poll can miss (heard as garbled high notes).
const PERIODS_PER_BUFFER: usize = 8;
const BUFFER_WORDS: usize = TONE_PATTERN.len() * PERIODS_PER_BUFFER;

const fn repeated_pattern() -> [u32; BUFFER_WORDS] {
    let mut out = [0u32; BUFFER_WORDS];
    let mut i = 0;
    while i < BUFFER_WORDS {
        out[i] = TONE_PATTERN[i % TONE_PATTERN.len()];
        i += 1;
    }
    out
}

static TONE_BUFFER: [u32; BUFFER_WORDS] = repeated_pattern();

pub struct Speaker {
    ssc: Ssc,
    pio: Pio,
}

impl Speaker {
    pub fn new(ssc_clock: SscClock, pioa_clock: PioClock) -> Self {
        let ssc = Ssc::new(ssc_clock);
        let r = ssc.regs();
        r.idr.set(!0);
        r.cr.set(ssc::CR_SWRST);
        r.tcmr
            .set(ssc::TCMR_CKS_DIV | ssc::TCMR_CKO_CONTINUOUS | ssc::TCMR_START_CONTINUOUS);
        r.tfmr
            .set(31 | ssc::TFMR_DATDEF | ssc::TFMR_MSBF | (7 << 8));
        let mut pio = Pio::new(pioa_clock);
        pio.park_low(PA17_TD);
        Self { ssc, pio }
    }

    /// Blocking beep. The tone is streamed by the SSC's DMA (PDC), chained
    /// one 16-word cycle at a time, so `background` (e.g. the AVR link
    /// service, which can block for ~0.3 ms) may take as long as it likes
    /// per call without starving the output -- feeding the FIFO by hand
    /// from the CPU underran and sounded like white noise. `background`
    /// must still return within one cycle (~1-2 ms).
    ///
    /// NxOS asserts `ms >= 100` (shorter beeps crashed its driver; kept as
    /// a guard).
    pub fn beep(&mut self, freq: u32, ms: u32, mut background: impl FnMut()) {
        assert!(freq > 0 && ms >= 100);
        let r = self.ssc.regs();
        // Bit clock = MCK / (2 * CMR) and one tone period is 16 * 32 = 512
        // bits, so CMR = MCK / (1024 * freq). MCK is the 96.11 MHz PLL / 2
        // = 48.05 MHz; NxOS's constant (96.11 MHz) omits that halving,
        // which put every note an octave low here.
        r.cmr.set(((48_054_857 / 1024) + freq / 2) / freq);
        // Buffers of PERIODS_PER_BUFFER periods each (rounded to nearest, so a
        // tone is within half a buffer of `ms`).
        let periods = (freq * ms) / 1000;
        let cycles = (periods + PERIODS_PER_BUFFER as u32 / 2) / PERIODS_PER_BUFFER as u32;
        if cycles == 0 {
            return;
        }
        // Hand PA17 to the SSC only while a tone plays.
        self.pio.assign_peripheral_a(PA17_TD);
        r.cr.set(ssc::CR_TXEN);
        let pattern = TONE_BUFFER.as_ptr() as u32;
        let words = BUFFER_WORDS as u32;

        r.tpr.set(pattern);
        r.tcr.set(words);
        r.tnpr.set(pattern);
        r.tncr.set(words);
        r.ptcr.set(ssc::PTCR_TXTEN);
        let mut queued = 2.min(cycles);
        let mut spins = 0u32;
        while queued < cycles {
            background();
            if r.sr.get() & ssc::SR_ENDTX != 0 {
                // Current buffer finished; the chained one is now playing.
                r.tnpr.set(pattern);
                r.tncr.set(words);
                queued += 1;
                spins = 0;
            } else {
                spins += 1;
                if spins > 5_000_000 {
                    break; // DMA not advancing; give up rather than hang
                }
            }
        }
        // Let the last queued buffers drain, then stop the DMA.
        let mut spins = 0u32;
        while r.sr.get() & ssc::SR_TXBUFE == 0 {
            background();
            spins += 1;
            if spins > 5_000_000 {
                break;
            }
        }
        r.ptcr.set(ssc::PTCR_TXTDIS);
        // Idle the amplifier input: with the SSC left on, TD sits at a DC
        // high level and the speaker hissed constantly.
        r.cr.set(ssc::CR_TXDIS);
        self.pio.park_low(PA17_TD);
    }
}

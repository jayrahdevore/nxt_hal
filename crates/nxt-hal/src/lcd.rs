//! 100x64 monochrome LCD (UC1601 controller) over SPI, ported from NxOS
//! `base/drivers/_lcd.c` (polled, no PDC/interrupts).
//!
//! The framebuffer is 8 pages x 100 columns; each byte holds 8 vertically
//! adjacent pixels of one column (bit 0 = top), 1 = dark. The panel is
//! write-only (MISO is wired to the command/data line), so nothing here can
//! be read back. Requires `Pmc::init_clocks` to have run (2 MHz SPI clock
//! assumes MCK ~= 48 MHz).

use nxt_pac::pio::{ClockEnabled as PioClock, Pio};
use nxt_pac::pit::Pit;
use nxt_pac::spi::{ClockEnabled as SpiClock, Spi, PA10_NPCS2, PA12_MISO};

pub const WIDTH: usize = 100;
pub const HEIGHT: usize = 64;
pub const PAGES: usize = HEIGHT / 8;
pub const FRAME_BYTES: usize = WIDTH * PAGES;

/// UC1601 command bytes, computed per NxOS's macros.
const RESET: u8 = 0xE2;
const INIT_SEQUENCE: [u8; 10] = [
    0x23, // multiplex rate 1/65
    0xEB, // bias ratio 1/9
    0x81, // set potentiometer...
    92,   // ...to 92 (9 V V(LCD) per NxOS)
    0x89, // RAM address control: auto-wrap, row-major
    0xC4, // map control: Y mirror (display is mounted upside down)
    0x00, // column address low 0
    0x10, // column address high 0
    0xB0, // page 0
    0xAF, // display on
];

pub struct Lcd {
    spi: Spi,
    pio: Pio,
}

impl Lcd {
    /// Brings the controller out of reset and turns the display on.
    /// `service` is called repeatedly during the two 20 ms settle waits so
    /// the caller can keep the AVR link alive.
    pub fn new(
        spi_clock: SpiClock,
        pioa_clock: PioClock,
        pit: &Pit,
        mut service: impl FnMut(),
    ) -> Self {
        let mut pio = Pio::new(pioa_clock);
        let spi = Spi::new(spi_clock, &mut pio);
        let mut lcd = Self { spi, pio };

        // Select the controller. NxOS forces this by hand because letting
        // the SPI controller drive NPCS2 fails.
        lcd.pio.set_low(PA10_NPCS2);
        delay_ms(pit, 20, &mut service);

        lcd.command(&[RESET]);
        delay_ms(pit, 20, &mut service);

        lcd.command(&INIT_SEQUENCE);
        lcd
    }

    /// Pushes a full frame to the panel. Blocking, ~0.5 ms at 2 MHz.
    pub fn flush(&mut self, frame: &[u8; FRAME_BYTES]) {
        for (page, columns) in frame.chunks_exact(WIDTH).enumerate() {
            self.command(&[0x00, 0x10, 0xB0 | page as u8]);
            self.data(columns);
        }
    }

    /// Puts the controller into reset ahead of a power-off. The spec says
    /// this lets the board capacitors drain through the controller's drain
    /// circuit instead of discharging uncontrolled (NxOS `nx__lcd_shutdown`).
    /// `service` is called during the 20 ms wait to keep the AVR link alive.
    pub fn shutdown(&mut self, pit: &Pit, mut service: impl FnMut()) {
        self.command(&[RESET]);
        delay_ms(pit, 20, &mut service);
    }

    /// Blanks the whole panel (display stays on).
    pub fn clear(&mut self) {
        for page in 0..PAGES {
            self.command(&[0x00, 0x10, 0xB0 | page as u8]);
            self.data(&[0; WIDTH]);
        }
    }

    fn command(&mut self, bytes: &[u8]) {
        self.set_mode(false);
        for &b in bytes {
            self.spi.write_byte(b);
        }
        self.spi.flush();
    }

    fn data(&mut self, bytes: &[u8]) {
        self.set_mode(true);
        for &b in bytes {
            self.spi.write_byte(b);
        }
        self.spi.flush();
    }

    /// Drives the command/data line (high = data). The shifter must be
    /// empty first or the last byte of the previous mode is misclassified.
    fn set_mode(&mut self, data: bool) {
        self.spi.flush();
        if data {
            self.pio.set_high(PA12_MISO);
        } else {
            self.pio.set_low(PA12_MISO);
        }
    }
}

/// Busy-waits `ms` PIT ticks, calling `service` on every spin.
fn delay_ms(pit: &Pit, ms: u32, service: &mut impl FnMut()) {
    let mut last = pit.ticks();
    let mut elapsed = 0;
    while elapsed < ms {
        service();
        let now = pit.ticks();
        // PICNT is 12 bits wide and wraps.
        elapsed += now.wrapping_sub(last) & 0xFFF;
        last = now;
    }
}

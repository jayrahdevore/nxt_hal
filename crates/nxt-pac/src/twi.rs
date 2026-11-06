//! Two-Wire Interface (TWI / I2C master) -- the ARM<->AVR link.
//!
//! Base address, register offsets and bit positions: base/ID/PA3-PA4
//! are verbatim from NxOS's `at91sam7s256.h`; the register offsets and bit
//! masks came from a model-written summary of that header (unverified,
//! but consistent with the SAM7S datasheet layout as I remember it).
//! Polled (no interrupts); flow follows NxOS `_twi.c`.

use crate::pio::Pio;
use vcell::VolatileCell;

#[repr(C)]
pub struct RegisterBlock {
    pub cr: VolatileCell<u32>,   // 0x00
    pub mmr: VolatileCell<u32>,  // 0x04
    _reserved0: u32,             // 0x08
    pub iadr: VolatileCell<u32>, // 0x0C
    pub cwgr: VolatileCell<u32>, // 0x10
    _reserved1: [u32; 3],        // 0x14..0x1C
    pub sr: VolatileCell<u32>,   // 0x20
    pub ier: VolatileCell<u32>,  // 0x24
    pub idr: VolatileCell<u32>,  // 0x28
    pub imr: VolatileCell<u32>,  // 0x2C
    pub rhr: VolatileCell<u32>,  // 0x30
    pub thr: VolatileCell<u32>,  // 0x34
}

/// Peripheral ID of the TWI (`AT91C_ID_TWI`).
pub const ID_TWI: u32 = 9;

const CR_START: u32 = 1 << 0;
const CR_STOP: u32 = 1 << 1;
const CR_MSEN: u32 = 1 << 2;
const CR_MSDIS: u32 = 1 << 3;
const CR_SWRST: u32 = 1 << 7;
const MMR_MREAD: u32 = 1 << 12;
const SR_TXCOMP: u32 = 1 << 0;
const SR_RXRDY: u32 = 1 << 1;
const SR_TXRDY: u32 = 1 << 2;
const SR_OVRE: u32 = 1 << 6;
const SR_UNRE: u32 = 1 << 7;
const SR_NACK: u32 = 1 << 8;

const PA3_TWD: u32 = 1 << 3;
const PA4_TWCK: u32 = 1 << 4;

/// Spin limit for every status wait, so a dead bus errors out instead of
/// hanging the firmware.
const SPIN_LIMIT: u32 = 50_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Nack,
    Timeout,
    Bus,
}

/// Proof that `Pmc::enable_twi_clock` has run.
pub struct ClockEnabled(pub(crate) ());

pub struct Twi {
    _private: (),
    /// Debug: last SR seen and byte index at the most recent failure.
    pub dbg_sr: u32,
    pub dbg_idx: u32,
}

impl Twi {
    const PTR: *mut RegisterBlock = 0xFFFB_8000 as *mut RegisterBlock;

    /// Recovers a possibly-stuck bus, then configures the controller
    /// (~380 kHz, per NxOS). The AVR may still believe it is mid-
    /// transaction after a warm reboot and hold SDA low; the TWI hardware
    /// locks up if initialised in that state, so PA3/PA4 are first driven
    /// by hand for up to 9 clocks until SDA releases.
    pub fn new(_clock: ClockEnabled, pio: &mut Pio) -> Self {
        let twi = Self {
            _private: (),
            dbg_sr: 0,
            dbg_idx: 0,
        };
        twi.regs().idr.set(!0);

        let p = pio.regs();
        p.mder.set(PA3_TWD | PA4_TWCK);
        p.per.set(PA3_TWD | PA4_TWCK);
        p.odr.set(PA3_TWD);
        p.oer.set(PA4_TWCK);
        let mut clocks = 9;
        while clocks > 0 && p.pdsr.get() & PA3_TWD == 0 {
            p.codr.set(PA4_TWCK);
            spin(100);
            p.sodr.set(PA4_TWCK);
            spin(100);
            clocks -= 1;
        }
        p.asr.set(PA3_TWD | PA4_TWCK);
        p.pdr.set(PA3_TWD | PA4_TWCK);

        let r = twi.regs();
        r.cr.set(CR_SWRST | CR_MSDIS);
        r.cwgr.set(0x020f0f);
        r.cr.set(CR_MSEN);
        twi
    }

    fn regs(&self) -> &'static RegisterBlock {
        unsafe { &*Self::PTR }
    }

    /// Waits for any of `mask` in SR. `acc` accumulates SR reads because
    /// reading SR clears NACK/OVRE/UNRE.
    fn wait(&mut self, mask: u32, acc: &mut u32) -> Result<(), Error> {
        for _ in 0..SPIN_LIMIT {
            *acc |= self.regs().sr.get();
            if *acc & SR_NACK != 0 {
                return Err(Error::Nack);
            }
            if *acc & (SR_OVRE | SR_UNRE) != 0 {
                return Err(Error::Bus);
            }
            if *acc & mask != 0 {
                return Ok(());
            }
        }
        self.dbg_sr = *acc;
        Err(Error::Timeout)
    }

    pub fn write(&mut self, addr: u8, data: &[u8]) -> Result<(), Error> {
        let r = self.regs();
        let mut acc = 0;
        r.mmr.set((addr as u32 & 0x7f) << 16);
        r.cr.set(CR_START | CR_MSEN);
        for (i, b) in data.iter().enumerate() {
            self.dbg_idx = i as u32;
            self.wait_or_abort(SR_TXRDY, &mut acc)?;
            acc &= !SR_TXRDY;
            if i + 1 == data.len() {
                r.cr.set(CR_STOP);
            }
            r.thr.set(*b as u32);
        }
        // Fresh view: TXCOMP is also set while idle, so a stale accumulated
        // bit would end the wait before the transfer has really finished.
        acc = 0;
        self.wait_or_abort(SR_TXCOMP, &mut acc)
    }

    pub fn read(&mut self, addr: u8, buf: &mut [u8]) -> Result<(), Error> {
        let r = self.regs();
        let mut acc = 0;
        r.mmr.set(((addr as u32 & 0x7f) << 16) | MMR_MREAD);
        r.cr.set(CR_START | CR_MSEN);
        let n = buf.len();
        if n == 1 {
            r.cr.set(CR_STOP);
        }
        for i in 0..n {
            self.dbg_idx = 100 + i as u32;
            self.wait_or_abort(SR_RXRDY, &mut acc)?;
            acc &= !SR_RXRDY;
            buf[i] = r.rhr.get() as u8;
            if i + 2 == n {
                r.cr.set(CR_STOP);
            }
        }
        self.dbg_idx = 100 + n as u32;
        acc = 0;
        self.wait_or_abort(SR_TXCOMP, &mut acc)
    }

    fn wait_or_abort(&mut self, mask: u32, acc: &mut u32) -> Result<(), Error> {
        self.wait(mask, acc).map_err(|e| {
            self.regs().cr.set(CR_STOP);
            e
        })
    }
}

fn spin(n: u32) {
    for _ in 0..n {
        core::hint::black_box(());
    }
}

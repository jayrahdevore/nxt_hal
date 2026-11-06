//! Power Management Controller (PMC).
//!
//! Base address and register offsets from Atmel's `AT91SAM7S256.h`
//! (`AT91C_BASE_PMC = 0xFFFFFC00`). Only the peripheral-clock-gating
//! registers (`PCER`/`PCDR`/`PCSR`) are exposed so far — main
//! oscillator/PLL/MCK bring-up (`CKGR_MOR`, `CKGR_PLLR`, `MCKR`) is deferred
//! until the exact recipe is confirmed against a reference implementation
//! (see `docs/adr/0001-motor-avr-link.md` for the tracking note), since
//! Milestone 0 only needs the default slow-clock to toggle a GPIO pin.

use crate::pio::ClockEnabled;
use vcell::VolatileCell;

#[repr(C)]
pub struct RegisterBlock {
    pub scer: VolatileCell<u32>, // 0x00 PMC_SCER: System Clock Enable
    pub scdr: VolatileCell<u32>, // 0x04 PMC_SCDR: System Clock Disable
    pub scsr: VolatileCell<u32>, // 0x08 PMC_SCSR: System Clock Status
    _reserved0: u32,             // 0x0C
    pub pcer: VolatileCell<u32>, // 0x10 PMC_PCER: Peripheral Clock Enable
    pub pcdr: VolatileCell<u32>, // 0x14 PMC_PCDR: Peripheral Clock Disable
    pub pcsr: VolatileCell<u32>, // 0x18 PMC_PCSR: Peripheral Clock Status
    _reserved1: u32,             // 0x1C
    pub ckgr_mor: VolatileCell<u32>,  // 0x20 CKGR_MOR: Main Oscillator Register
    _reserved2: [u32; 2],             // 0x24, 0x28
    pub ckgr_pllr: VolatileCell<u32>, // 0x2C CKGR_PLLR: PLL Register
    pub mckr: VolatileCell<u32>,      // 0x30 PMC_MCKR: Master Clock Register
    _reserved3: [u32; 13],            // 0x34..=0x64 (includes PMC_IER/IDR, unused for now)
    pub sr: VolatileCell<u32>,        // 0x68 PMC_SR: Status Register
}

/// Peripheral Identifier for PIOA (`AT91C_ID_PIOA`), i.e. its bit position
/// in `PCER`/`PCDR`/`PCSR`.
pub const ID_PIOA: u32 = 2;

// Bring-up recipe and constants from NxOS `base/init.S`: 18.432 MHz
// crystal, PLL /14 *73 = 96.11 MHz, MCK = PLL/2 ~= 48 MHz.
const MC_FMR: *mut u32 = 0xFFFF_FF60 as *mut u32;
const SR_MOSCS: u32 = 1 << 0;
const SR_LOCK: u32 = 1 << 2;
const SR_MCKRDY: u32 = 1 << 3;

/// Owns the PMC register block.
pub struct Pmc {
    _private: (),
}

impl Pmc {
    const PTR: *mut RegisterBlock = 0xFFFF_FC00 as *mut RegisterBlock;

    /// # Safety
    /// Must be constructed at most once; see [`crate::Peripherals::take`].
    pub(crate) unsafe fn new() -> Self {
        Self { _private: () }
    }

    fn regs(&self) -> &RegisterBlock {
        unsafe { &*Self::PTR }
    }

    /// Enables PIOA's peripheral clock and returns a proof token that
    /// `nxt_pac::pio::Pio::new` requires. A peripheral with its clock bit
    /// unset reads back all-zero and ignores writes, so this ordering is
    /// enforced at compile time rather than left as a runtime footgun.
    pub fn enable_pioa_clock(&mut self) -> ClockEnabled {
        self.regs().pcer.set(1 << ID_PIOA);
        ClockEnabled(())
    }

    /// Enables the SSC's peripheral clock and returns the proof token
    /// `crate::ssc::Ssc::new` requires.
    pub fn enable_ssc_clock(&mut self) -> crate::ssc::ClockEnabled {
        self.regs().pcer.set(1 << crate::ssc::ID_SSC);
        crate::ssc::ClockEnabled(())
    }

    /// Enables the TWI's peripheral clock.
    pub fn enable_twi_clock(&mut self) -> crate::twi::ClockEnabled {
        self.regs().pcer.set(1 << crate::twi::ID_TWI);
        crate::twi::ClockEnabled(())
    }

    /// Switches MCK from the slow clock to the PLL (~48 MHz). Sets flash
    /// wait states first. Busy-waits on PMC_SR with no timeout, as NxOS
    /// does. Unverified on hardware.
    pub fn init_clocks(&mut self) {
        let r = self.regs();
        unsafe { MC_FMR.write_volatile((72 << 16) | (1 << 8)) }; // FMCN=72, FWS=1
        r.ckgr_mor.set(0x0000_0601); // MOSCEN, OSCOUNT=6
        while r.sr.get() & SR_MOSCS == 0 {}
        r.ckgr_pllr.set(0x1048_1C0E); // DIV=14 MUL=72 PLLCOUNT=28 OUT=1 USBDIV=/2
        while r.sr.get() & SR_LOCK == 0 {}
        while r.sr.get() & SR_MCKRDY == 0 {}
        r.mckr.set(0x4); // prescaler /2, still slow clock
        while r.sr.get() & SR_MCKRDY == 0 {}
        r.mckr.set(0x7); // source = PLL
        while r.sr.get() & SR_MCKRDY == 0 {}
    }
}

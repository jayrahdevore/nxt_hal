//! Watchdog Timer Controller (WDTC).
//!
//! Base address and register offsets from Atmel's `AT91SAM7S256.h`
//! (`AT91C_BASE_WDTC = 0xFFFFFD40`). The watchdog is **enabled by default
//! out of reset** with a short timeout, so disabling (or servicing) it is
//! the first thing firmware must do.

use vcell::VolatileCell;

#[repr(C)]
pub struct RegisterBlock {
    pub wdcr: VolatileCell<u32>, // 0x00 WDTC_WDCR: Control Register
    pub wdmr: VolatileCell<u32>, // 0x04 WDTC_WDMR: Mode Register
    pub wdsr: VolatileCell<u32>, // 0x08 WDTC_WDSR: Status Register
}

/// WDTC_WDMR: Watchdog Disable (bit 15).
pub const WDDIS: u32 = 1 << 15;

/// Owns the WDTC register block.
pub struct Wdt {
    _private: (),
}

impl Wdt {
    const PTR: *mut RegisterBlock = 0xFFFF_FD40 as *mut RegisterBlock;

    /// # Safety
    /// Must be constructed at most once; see [`crate::Peripherals::take`].
    pub(crate) unsafe fn new() -> Self {
        Self { _private: () }
    }

    fn regs(&self) -> &RegisterBlock {
        unsafe { &*Self::PTR }
    }

    /// Permanently disables the watchdog. Once `WDDIS` is set it cannot be
    /// cleared again until the next reset (per WDTC_WDMR's documented
    /// behavior), so there is no corresponding `enable`.
    pub fn disable(&mut self) {
        self.regs().wdmr.set(WDDIS);
    }
}

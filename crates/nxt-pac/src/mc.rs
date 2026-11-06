//! Memory Controller (MC): remap register and the embedded flash
//! controller (EFC) registers.
//!
//! Base `0xFFFFFF00` (`AT91C_BASE_MC`), layout from `AT91S_MC` in
//! `at91sam7s256.h`. The MC has no PMC clock gate.

use vcell::VolatileCell;

#[repr(C)]
pub struct RegisterBlock {
    pub rcr: VolatileCell<u32>,  // 0x00 MC_RCR: Remap Control
    pub asr: VolatileCell<u32>,  // 0x04 MC_ASR: Abort Status
    pub aasr: VolatileCell<u32>, // 0x08 MC_AASR: Abort Address Status
    _reserved0: [u32; 21],       // 0x0C..=0x5C
    pub fmr: VolatileCell<u32>,  // 0x60 MC_FMR: Flash Mode
    pub fcr: VolatileCell<u32>,  // 0x64 MC_FCR: Flash Command
    pub fsr: VolatileCell<u32>,  // 0x68 MC_FSR: Flash Status
}

/// MC_FSR: flash ready. Cleared while a command runs.
pub const FSR_FRDY: u32 = 1 << 0;
/// MC_FSR: lock error (write to a locked region). Cleared on FSR read.
pub const FSR_LOCKE: u32 = 1 << 2;
/// MC_FSR: programming error. Cleared on FSR read.
pub const FSR_PROGE: u32 = 1 << 3;
/// MC_FSR: lock status of region `n` (0..16) is `1 << (FSR_LOCKS_SHIFT + n)`.
pub const FSR_LOCKS_SHIFT: u32 = 16;

/// MC_FMR: flash wait states field position (`AT91C_MC_FWS`, 2 bits).
pub const FMR_FWS_SHIFT: u32 = 8;
/// MC_FMR: flash microsecond cycle number field position (`AT91C_MC_FMCN`, 8 bits).
pub const FMR_FMCN_SHIFT: u32 = 16;

/// MC_FCR: write-protect key, bits 31:24.
pub const FCR_KEY: u32 = 0x5A << 24;
/// MC_FCR: page number field position (`AT91C_MC_PAGEN`, 10 bits, bits 17:8).
/// For lock/unlock commands, bits 7:4 of PAGEN select the region.
pub const FCR_PAGEN_SHIFT: u32 = 8;

/// MC_FCR command codes (`AT91C_MC_FCMD`, bits 3:0).
pub mod cmd {
    /// Program the latched page (erases it first).
    pub const WRITE_PAGE: u32 = 0x1;
    /// Set lock bit of the region containing the page.
    pub const SET_LOCK: u32 = 0x2;
    /// Clear lock bit of the region containing the page.
    pub const CLEAR_LOCK: u32 = 0x4;
}

/// Owns the MC register block.
pub struct Mc {
    _private: (),
}

impl Mc {
    const PTR: *mut RegisterBlock = 0xFFFF_FF00 as *mut RegisterBlock;

    /// # Safety
    /// Must be constructed at most once; see [`crate::Peripherals::take`].
    pub(crate) unsafe fn new() -> Self {
        Self { _private: () }
    }

    pub fn regs(&self) -> &RegisterBlock {
        unsafe { &*Self::PTR }
    }
}

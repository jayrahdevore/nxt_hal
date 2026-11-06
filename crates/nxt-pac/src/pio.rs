//! Parallel I/O Controller A (PIOA) — the NXT's only PIO bank, 32 lines
//! (PA0..PA31).
//!
//! Base address and register offsets from Atmel's `AT91SAM7S256.h`
//! (`AT91C_BASE_PIOA = 0xFFFFF400`).

use vcell::VolatileCell;

#[repr(C)]
pub struct RegisterBlock {
    pub per: VolatileCell<u32>, // 0x00 PIO Enable (claim lines for PIO vs. a peripheral function)
    pub pdr: VolatileCell<u32>, // 0x04 PIO Disable
    pub psr: VolatileCell<u32>, // 0x08 PIO Status
    _reserved0: u32,            // 0x0C
    pub oer: VolatileCell<u32>, // 0x10 Output Enable
    pub odr: VolatileCell<u32>, // 0x14 Output Disable
    pub osr: VolatileCell<u32>, // 0x18 Output Status
    _reserved1: u32,            // 0x1C
    pub ifer: VolatileCell<u32>, // 0x20 Glitch Input Filter Enable
    pub ifdr: VolatileCell<u32>, // 0x24 Glitch Input Filter Disable
    _reserved2: [u32; 2],        // 0x28, 0x2C
    pub sodr: VolatileCell<u32>, // 0x30 Set Output Data
    pub codr: VolatileCell<u32>, // 0x34 Clear Output Data
    pub odsr: VolatileCell<u32>, // 0x38 Output Data Status
    pub pdsr: VolatileCell<u32>, // 0x3C Pin Data Status (reads the actual pin level)
    _reserved3: [u32; 4],        // 0x40..=0x4C
    pub mder: VolatileCell<u32>, // 0x50 Multi-Driver (open-drain) Enable
    pub mddr: VolatileCell<u32>, // 0x54 Multi-Driver Disable
    _reserved4: [u32; 2],        // 0x58, 0x5C
    pub ppudr: VolatileCell<u32>, // 0x60 Pull-Up Disable
    pub ppuer: VolatileCell<u32>, // 0x64 Pull-Up Enable
    _reserved5: [u32; 2],         // 0x68, 0x6C
    pub asr: VolatileCell<u32>, // 0x70 Select Peripheral A
    pub bsr: VolatileCell<u32>, // 0x74 Select Peripheral B
}

/// Proof that `Pmc::enable_pioa_clock` has run. PIOA's registers are
/// otherwise unresponsive (reads return zero, writes are ignored), so
/// [`Pio::new`] requires one of these rather than letting that be a
/// silent runtime trap.
pub struct ClockEnabled(pub(crate) ());

/// Owns the PIOA register block.
pub struct Pio {
    _private: (),
}

impl Pio {
    const PTR: *mut RegisterBlock = 0xFFFF_F400 as *mut RegisterBlock;

    pub fn new(_clock: ClockEnabled) -> Self {
        Self { _private: () }
    }

    pub fn regs(&self) -> &RegisterBlock {
        unsafe { &*Self::PTR }
    }

    /// Claims `mask` (bitmask over PA0..PA31) for direct PIO control
    /// (rather than a multiplexed peripheral function) and configures
    /// those lines as outputs.
    pub fn enable_output(&mut self, mask: u32) {
        self.regs().per.set(mask);
        self.regs().oer.set(mask);
    }

    /// Hands `mask`'s lines to peripheral A (e.g. PA17 -> SSC TD).
    pub fn assign_peripheral_a(&mut self, mask: u32) {
        self.regs().asr.set(mask);
        self.regs().pdr.set(mask);
    }

    /// Takes `mask`'s lines back from their peripheral and drives them low.
    pub fn park_low(&mut self, mask: u32) {
        self.regs().codr.set(mask);
        self.enable_output(mask);
    }

    /// Drives `mask`'s lines high.
    pub fn set_high(&mut self, mask: u32) {
        self.regs().sodr.set(mask);
    }

    /// Drives `mask`'s lines low.
    pub fn set_low(&mut self, mask: u32) {
        self.regs().codr.set(mask);
    }

    /// Reads the current level of `mask`'s lines (masked, not shifted).
    pub fn read_input(&self, mask: u32) -> u32 {
        self.regs().pdsr.get() & mask
    }
}

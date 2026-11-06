//! Synchronous Serial Controller (SSC) -- on the NXT it drives the sound
//! amplifier via TD (PA17).
//!
//! Peripheral ID 8 is verified (NxOS header); base address (`AT91C_BASE_SSC = 0xFFFD4000`), offsets and bit positions
//! are from memory of Atmel's `AT91SAM7S256.h` and the register order in
//! NxOS's `sound.c`; they were NOT cross-checked against the header text
//! (the NxOS fetch truncated it). Verify before trusting.

use vcell::VolatileCell;

#[repr(C)]
pub struct RegisterBlock {
    pub cr: VolatileCell<u32>,  // 0x00 Control
    pub cmr: VolatileCell<u32>, // 0x04 Clock Mode (divider)
    _reserved0: [u32; 2],       // 0x08, 0x0C
    pub rcmr: VolatileCell<u32>, // 0x10
    pub rfmr: VolatileCell<u32>, // 0x14
    pub tcmr: VolatileCell<u32>, // 0x18 Transmit Clock Mode
    pub tfmr: VolatileCell<u32>, // 0x1C Transmit Frame Mode
    pub rhr: VolatileCell<u32>,  // 0x20
    pub thr: VolatileCell<u32>,  // 0x24 Transmit Holding
    _reserved1: [u32; 2],        // 0x28, 0x2C
    pub rshr: VolatileCell<u32>, // 0x30
    pub tshr: VolatileCell<u32>, // 0x34
    _reserved2: [u32; 2],        // 0x38, 0x3C
    pub sr: VolatileCell<u32>,   // 0x40 Status
    pub ier: VolatileCell<u32>,  // 0x44
    pub idr: VolatileCell<u32>,  // 0x48 Interrupt Disable
    pub imr: VolatileCell<u32>,  // 0x4C
    _reserved3: [u32; 44],       // 0x50..0xFC
    // PDC (DMA) registers; offsets from memory of the SAM7S layout, TNPR/TNCR/
    // PTCR usage confirmed by NxOS sound.c.
    pub rpr: VolatileCell<u32>,  // 0x100
    pub rcr: VolatileCell<u32>,  // 0x104
    pub tpr: VolatileCell<u32>,  // 0x108 Transmit Pointer
    pub tcr: VolatileCell<u32>,  // 0x10C Transmit Counter
    pub rnpr: VolatileCell<u32>, // 0x110
    pub rncr: VolatileCell<u32>, // 0x114
    pub tnpr: VolatileCell<u32>, // 0x118 Transmit Next Pointer
    pub tncr: VolatileCell<u32>, // 0x11C Transmit Next Counter
    pub ptcr: VolatileCell<u32>, // 0x120 PDC Transfer Control
    pub ptsr: VolatileCell<u32>, // 0x124 PDC Transfer Status
}

/// Peripheral ID of the SSC (`AT91C_ID_SSC`), its bit in PMC `PCER`.
pub const ID_SSC: u32 = 8;

pub const CR_TXEN: u32 = 1 << 8;
pub const CR_TXDIS: u32 = 1 << 9;
pub const CR_SWRST: u32 = 1 << 15;
pub const SR_TXRDY: u32 = 1 << 0;
pub const SR_ENDTX: u32 = 1 << 2;
pub const SR_TXBUFE: u32 = 1 << 3;
pub const PTCR_TXTEN: u32 = 1 << 8;
pub const PTCR_TXTDIS: u32 = 1 << 9;
pub const TCMR_CKS_DIV: u32 = 0;
pub const TCMR_CKO_CONTINUOUS: u32 = 1 << 2;
pub const TCMR_START_CONTINUOUS: u32 = 0;
pub const TFMR_DATDEF: u32 = 1 << 5;
pub const TFMR_MSBF: u32 = 1 << 7;

/// Proof that `Pmc::enable_ssc_clock` has run.
pub struct ClockEnabled(pub(crate) ());

pub struct Ssc {
    _private: (),
}

impl Ssc {
    const PTR: *mut RegisterBlock = 0xFFFD_4000 as *mut RegisterBlock;

    pub fn new(_clock: ClockEnabled) -> Self {
        Self { _private: () }
    }

    pub fn regs(&self) -> &RegisterBlock {
        unsafe { &*Self::PTR }
    }
}

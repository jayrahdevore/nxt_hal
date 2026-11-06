//! USB Device Port (UDP, full-speed device controller).
//!
//! Base `0xFFFB0000` (`AT91C_BASE_UDP`), layout and bits from `AT91S_UDP`
//! and `AT91C_UDP_*` in `at91sam7s256.h`. Peripheral ID 11; also needs the
//! UDP system clock (`PMC_SCER` bit 7) -- see `Pmc::enable_udp_clock`.
//! The D+ pull-up is *not* in this block: on the NXT it is PA16, active low.

use vcell::VolatileCell;

#[repr(C)]
pub struct RegisterBlock {
    pub num: VolatileCell<u32>,      // 0x00 UDP_NUM: Frame Number
    pub glbstate: VolatileCell<u32>, // 0x04 UDP_GLBSTATE: Global State
    pub faddr: VolatileCell<u32>,    // 0x08 UDP_FADDR: Function Address
    _reserved0: u32,                 // 0x0C
    pub ier: VolatileCell<u32>,      // 0x10 UDP_IER: Interrupt Enable
    pub idr: VolatileCell<u32>,      // 0x14 UDP_IDR: Interrupt Disable
    pub imr: VolatileCell<u32>,      // 0x18 UDP_IMR: Interrupt Mask
    pub isr: VolatileCell<u32>,      // 0x1C UDP_ISR: Interrupt Status
    pub icr: VolatileCell<u32>,      // 0x20 UDP_ICR: Interrupt Clear
    _reserved1: u32,                 // 0x24
    pub rstep: VolatileCell<u32>,    // 0x28 UDP_RSTEP: Reset Endpoint
    _reserved2: u32,                 // 0x2C
    pub csr: [VolatileCell<u32>; 8], // 0x30 UDP_CSR[n]: Endpoint Control and Status
    pub fdr: [VolatileCell<u32>; 8], // 0x50 UDP_FDR[n]: Endpoint FIFO Data
    _reserved3: u32,                 // 0x70
    pub txvc: VolatileCell<u32>,     // 0x74 UDP_TXVC: Transceiver Control
}

/// Peripheral ID of the UDP (`AT91C_ID_UDP`).
pub const ID_UDP: u32 = 11;

// UDP_GLBSTATE
pub const GLB_FADDEN: u32 = 1 << 0;
pub const GLB_CONFG: u32 = 1 << 1;

// UDP_FADDR
pub const FADDR_FEN: u32 = 1 << 8;

// UDP_ISR / IER / IDR / IMR / ICR
pub const INT_EP0: u32 = 1 << 0; // EPINTn is 1 << n
pub const INT_RXSUSP: u32 = 1 << 8;
pub const INT_RXRSM: u32 = 1 << 9;
pub const INT_EXTRSM: u32 = 1 << 10;
pub const INT_SOFINT: u32 = 1 << 11;
pub const INT_ENDBUSRES: u32 = 1 << 12;
pub const INT_WAKEUP: u32 = 1 << 13;

// UDP_CSR[n]
pub const CSR_TXCOMP: u32 = 1 << 0;
pub const CSR_RX_DATA_BK0: u32 = 1 << 1;
pub const CSR_RXSETUP: u32 = 1 << 2;
/// Control endpoints: stall sent (aliases `ISOERROR` for isochronous).
pub const CSR_STALLSENT: u32 = 1 << 3;
pub const CSR_TXPKTRDY: u32 = 1 << 4;
pub const CSR_FORCESTALL: u32 = 1 << 5;
pub const CSR_RX_DATA_BK1: u32 = 1 << 6;
pub const CSR_DIR: u32 = 1 << 7;
pub const CSR_EPTYPE_SHIFT: u32 = 8;
pub const CSR_EPTYPE_CTRL: u32 = 0x0 << 8;
pub const CSR_EPTYPE_BULK_OUT: u32 = 0x2 << 8;
pub const CSR_EPTYPE_BULK_IN: u32 = 0x6 << 8;
pub const CSR_DTGLE: u32 = 1 << 11;
pub const CSR_EPEDS: u32 = 1 << 15;
pub const CSR_RXBYTECNT_SHIFT: u32 = 16;
pub const CSR_RXBYTECNT_MASK: u32 = 0x7FF << 16;

// UDP_TXVC
pub const TXVC_TXVDIS: u32 = 1 << 8;
pub const TXVC_PUON: u32 = 1 << 9;

/// Proof that `Pmc::enable_udp_clock` has run (peripheral *and* system clock).
pub struct ClockEnabled(pub(crate) ());

/// Owns the UDP register block.
pub struct Udp {
    _private: (),
}

impl Udp {
    const PTR: *mut RegisterBlock = 0xFFFB_0000 as *mut RegisterBlock;

    pub fn new(_clock: ClockEnabled) -> Self {
        Self { _private: () }
    }

    pub fn regs(&self) -> &RegisterBlock {
        unsafe { &*Self::PTR }
    }
}

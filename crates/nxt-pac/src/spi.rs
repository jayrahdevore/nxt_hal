//! Serial Peripheral Interface (SPI master) -- the link to the UC1601 LCD
//! controller.
//!
//! Base address (0xFFFE0000), peripheral ID (5), register offsets and bit
//! positions are from NxOS's `at91sam7s256.h` (`AT91S_SPI`). The panel is
//! write-only: MISO (PA12) is wired to the UC1601's command/data line and
//! chip select (PA10, NPCS2) is driven by hand, so only SPCK (PA14) and
//! MOSI (PA13) belong to the SPI controller. Polled, no PDC; flow follows
//! NxOS `_lcd.c`.

use crate::pio::Pio;
use vcell::VolatileCell;

#[repr(C)]
pub struct RegisterBlock {
    pub cr: VolatileCell<u32>,     // 0x00
    pub mr: VolatileCell<u32>,     // 0x04
    pub rdr: VolatileCell<u32>,    // 0x08
    pub tdr: VolatileCell<u32>,    // 0x0C
    pub sr: VolatileCell<u32>,     // 0x10
    pub ier: VolatileCell<u32>,    // 0x14
    pub idr: VolatileCell<u32>,    // 0x18
    pub imr: VolatileCell<u32>,    // 0x1C
    _reserved0: [u32; 4],          // 0x20..0x2C
    pub csr: [VolatileCell<u32>; 4], // 0x30..0x3C
}

/// Peripheral ID of the SPI (`AT91C_ID_SPI`).
pub const ID_SPI: u32 = 5;

const CR_SPIEN: u32 = 1 << 0;
const CR_SWRST: u32 = 1 << 7;
const MR_MSTR: u32 = 1 << 0;
const MR_DLYBCS_SHIFT: u32 = 24;
const SR_TDRE: u32 = 1 << 1;
const SR_TXEMPTY: u32 = 1 << 9;
const CSR_CPOL: u32 = 1 << 0;
const CSR_BITS_8: u32 = 0 << 4;
const CSR_SCBR_SHIFT: u32 = 8;
const CSR_DLYBS_SHIFT: u32 = 16;
const CSR_DLYBCT_SHIFT: u32 = 24;

pub const PA10_NPCS2: u32 = 1 << 10;
pub const PA12_MISO: u32 = 1 << 12;
const PA13_MOSI: u32 = 1 << 13;
const PA14_SPCK: u32 = 1 << 14;

/// Proof that `Pmc::enable_spi_clock` has run.
pub struct ClockEnabled(pub(crate) ());

pub struct Spi {
    _private: (),
}

impl Spi {
    const PTR: *mut RegisterBlock = 0xFFFE_0000 as *mut RegisterBlock;

    /// Hands SPCK/MOSI to the controller, parks PA12 (command/data, low =
    /// command) and PA10 (chip select, high = deselected) as manual
    /// outputs, and configures master mode, 8 bits, CPOL=1 and
    /// SCBR=0x18 (48 MHz / 24 = 2 MHz) on CSR0. Only touches these four
    /// PIOA lines.
    pub fn new(_clock: ClockEnabled, pio: &mut Pio) -> Self {
        pio.assign_peripheral_a(PA13_MOSI | PA14_SPCK);
        pio.set_low(PA12_MISO);
        pio.set_high(PA10_NPCS2);
        pio.enable_output(PA12_MISO | PA10_NPCS2);

        let spi = Self { _private: () };
        let r = spi.regs();
        r.cr.set(CR_SWRST);
        r.cr.set(CR_SPIEN);
        r.idr.set(!0);
        r.mr.set((6 << MR_DLYBCS_SHIFT) | MR_MSTR);
        r.csr[0].set(
            (0x18 << CSR_DLYBCT_SHIFT)
                | (0x18 << CSR_DLYBS_SHIFT)
                | (0x18 << CSR_SCBR_SHIFT)
                | CSR_BITS_8
                | CSR_CPOL,
        );
        spi
    }

    fn regs(&self) -> &'static RegisterBlock {
        unsafe { &*Self::PTR }
    }

    /// Blocks until the transmit register is free, then queues `byte`.
    pub fn write_byte(&mut self, byte: u8) {
        while self.regs().sr.get() & SR_TDRE == 0 {}
        self.regs().tdr.set(byte as u32);
    }

    /// Blocks until the shifter has drained (every queued byte is on the
    /// wire). Needed before flipping a manually driven line such as the
    /// UC1601's command/data select.
    pub fn flush(&mut self) {
        while self.regs().sr.get() & SR_TXEMPTY == 0 {}
    }
}

//! Polled USB full-speed device on the AT91SAM7S's UDP.
//!
//! One vendor-specific interface: EP0 control, EP1 bulk OUT, EP2 bulk IN
//! (64-byte packets). Call [`Usb::poll`] often (every loop iteration);
//! there are no interrupts. Enumeration flow follows the datasheet and
//! NxOS's `usb.c`; the code is independent.
//!
//! The D+ pull-up is PA16, active low (drive low = connected to the host).
//!
//! Unverified on hardware.

use nxt_pac::pio::Pio;
use nxt_pac::pmc::Pmc;
use nxt_pac::udp::{self, Udp};

const PULLUP_PIN: u32 = 1 << 16; // PA16
const EP0_SIZE: usize = 8;
pub const BULK_SIZE: usize = 64;
const EP_OUT: usize = 1;
const EP_IN: usize = 2;
const SPIN_LIMIT: u32 = 100_000;

const REQ_GET_STATUS: u8 = 0;
const REQ_CLEAR_FEATURE: u8 = 1;
const REQ_SET_FEATURE: u8 = 3;
const REQ_SET_ADDRESS: u8 = 5;
const REQ_GET_DESCRIPTOR: u8 = 6;
const REQ_GET_CONFIGURATION: u8 = 8;
const REQ_SET_CONFIGURATION: u8 = 9;
const REQ_GET_INTERFACE: u8 = 10;
const REQ_SET_INTERFACE: u8 = 11;

const DESC_DEVICE: u8 = 1;
const DESC_CONFIG: u8 = 2;

/// Builds the device descriptor for `vid`/`pid`.
const fn device_descriptor(vid: u16, pid: u16) -> [u8; 18] {
    [
        18, DESC_DEVICE, 0x00, 0x02, // USB 2.0
        0xFF, 0x00, 0x00, // vendor-specific class
        EP0_SIZE as u8,
        vid as u8, (vid >> 8) as u8,
        pid as u8, (pid >> 8) as u8,
        0x00, 0x01, // device release 1.0
        0, 0, 0, // no strings
        1, // one configuration
    ]
}

static DEVICE_DESC: [u8; 18] = device_descriptor(nxt_boot_proto::USB_VID, nxt_boot_proto::USB_PID);

static CONFIG_DESC: [u8; 32] = [
    // configuration: 32 bytes total, 1 interface, self-powered
    9, DESC_CONFIG, 32, 0, 1, 1, 0, 0x40, 0,
    // interface 0: 2 endpoints, vendor-specific
    9, 4, 0, 0, 2, 0xFF, 0xFF, 0xFF, 0,
    // EP1 OUT, bulk, 64 bytes
    7, 5, 0x01, 0x02, BULK_SIZE as u8, 0, 0,
    // EP2 IN, bulk, 64 bytes
    7, 5, 0x82, 0x02, BULK_SIZE as u8, 0, 0,
];

static STATUS_SELF_POWERED: [u8; 2] = [1, 0];
static STATUS_ZERO: [u8; 2] = [0, 0];
static BYTE_ZERO: [u8; 1] = [0];
static BYTE_ONE: [u8; 1] = [1];

pub struct Usb {
    udp: Udp,
    pio: Pio,
    /// Remaining EP0 IN data stage.
    tx: &'static [u8],
    /// A zero-length packet must end the data stage (short of wLength, multiple of EP0_SIZE).
    tx_zlp: bool,
    /// Address to apply once the SET_ADDRESS status stage completes.
    pending_addr: Option<u8>,
    configured: bool,
    /// Next OUT bank to read: false = BK0, true = BK1.
    rx_bank1: bool,
}

impl Usb {
    /// Resets the controller with the pull-up off (host sees no device).
    pub fn new(pmc: &mut Pmc) -> Self {
        let clock = pmc.enable_udp_clock();
        let mut pio = Pio::new(pmc.enable_pioa_clock());
        // Level first, then output enable, so the line never glitches low.
        pio.set_high(PULLUP_PIN);
        pio.enable_output(PULLUP_PIN);
        let usb = Self {
            udp: Udp::new(clock),
            pio,
            tx: &[],
            tx_zlp: false,
            pending_addr: None,
            configured: false,
            rx_bank1: false,
        };
        let r = usb.udp.regs();
        r.idr.set(!0);
        r.rstep.set(0xFF);
        r.rstep.set(0);
        r.icr.set(!0);
        usb
    }

    /// Enables the D+ pull-up, announcing the device to the host.
    pub fn connect(&mut self) {
        self.pio.set_low(PULLUP_PIN);
    }

    /// Drops the pull-up so the host sees a disconnect.
    pub fn disconnect(&mut self) {
        self.pio.set_high(PULLUP_PIN);
    }

    /// True once the host has selected configuration 1.
    pub fn configured(&self) -> bool {
        self.configured
    }

    // CSR flag bits take effect asynchronously: after a write, spin until
    // the controller has synchronised the new value.
    fn csr_set(&self, ep: usize, flags: u32) {
        let c = &self.udp.regs().csr[ep];
        c.set(c.get() | flags);
        for _ in 0..SPIN_LIMIT {
            if c.get() & flags == flags {
                break;
            }
        }
    }

    fn csr_clear(&self, ep: usize, flags: u32) {
        let c = &self.udp.regs().csr[ep];
        c.set(c.get() & !flags);
        for _ in 0..SPIN_LIMIT {
            if c.get() & flags == 0 {
                break;
            }
        }
    }

    fn csr_write(&self, ep: usize, value: u32) {
        let c = &self.udp.regs().csr[ep];
        c.set(value);
        for _ in 0..SPIN_LIMIT {
            if c.get() == value {
                break;
            }
        }
    }

    fn bus_reset(&mut self) {
        let r = self.udp.regs();
        r.icr.set(!0);
        r.rstep.set(0xFF);
        r.rstep.set(0);
        self.tx = &[];
        self.tx_zlp = false;
        self.pending_addr = None;
        self.configured = false;
        self.rx_bank1 = false;
        self.csr_write(0, udp::CSR_EPEDS | udp::CSR_EPTYPE_CTRL);
        r.faddr.set(udp::FADDR_FEN);
        r.glbstate.set(0);
    }

    /// Services the controller. Cheap when idle.
    pub fn poll(&mut self) {
        let r = self.udp.regs();
        if r.isr.get() & udp::INT_ENDBUSRES != 0 {
            self.bus_reset();
            return;
        }
        // Housekeeping flags we don't act on.
        let stale = r.isr.get() & (udp::INT_RXSUSP | udp::INT_RXRSM | udp::INT_SOFINT | udp::INT_WAKEUP | udp::INT_EXTRSM);
        if stale != 0 {
            r.icr.set(stale);
        }

        let csr0 = r.csr[0].get();
        if csr0 & udp::CSR_STALLSENT != 0 {
            self.csr_clear(0, udp::CSR_FORCESTALL | udp::CSR_STALLSENT);
        }
        if csr0 & udp::CSR_RXSETUP != 0 {
            self.handle_setup();
        } else if csr0 & udp::CSR_TXCOMP != 0 {
            self.csr_clear(0, udp::CSR_TXCOMP);
            self.ep0_tx_complete();
        } else if csr0 & udp::CSR_RX_DATA_BK0 != 0 {
            // Zero-length OUT status stage of an IN control transfer.
            self.csr_clear(0, udp::CSR_RX_DATA_BK0);
        }

        // Bulk IN completion: just acknowledge; `write` checks TXPKTRDY.
        if self.udp.regs().csr[EP_IN].get() & udp::CSR_TXCOMP != 0 {
            self.csr_clear(EP_IN, udp::CSR_TXCOMP);
        }
    }

    fn ep0_send_next(&mut self) {
        let r = self.udp.regs();
        let n = self.tx.len().min(EP0_SIZE);
        for &b in &self.tx[..n] {
            r.fdr[0].set(b as u32);
        }
        self.tx = &self.tx[n..];
        self.csr_set(0, udp::CSR_TXPKTRDY);
    }

    fn ep0_tx_complete(&mut self) {
        if let Some(addr) = self.pending_addr.take() {
            let r = self.udp.regs();
            r.faddr.set(udp::FADDR_FEN | addr as u32);
            r.glbstate.set(if addr != 0 { udp::GLB_FADDEN } else { 0 });
        }
        if !self.tx.is_empty() {
            self.ep0_send_next();
        } else if self.tx_zlp {
            self.tx_zlp = false;
            self.ep0_send_next();
        }
    }

    /// Starts an EP0 IN data stage of at most `wlength` bytes of `data`.
    fn ep0_reply(&mut self, data: &'static [u8], wlength: usize) {
        let n = data.len().min(wlength);
        self.tx = &data[..n];
        self.tx_zlp = n < wlength && n != 0 && n % EP0_SIZE == 0;
        self.ep0_send_next();
    }

    /// Zero-length status/ack packet.
    fn ep0_ack(&mut self) {
        self.tx = &[];
        self.tx_zlp = false;
        self.ep0_send_next();
    }

    fn ep0_stall(&mut self) {
        self.csr_set(0, udp::CSR_FORCESTALL);
    }

    fn handle_setup(&mut self) {
        let r = self.udp.regs();
        let mut s = [0u8; 8];
        for b in s.iter_mut() {
            *b = r.fdr[0].get() as u8;
        }
        let (req_type, request) = (s[0], s[1]);
        let value = u16::from_le_bytes([s[2], s[3]]);
        let wlength = u16::from_le_bytes([s[6], s[7]]) as usize;

        // Direction must be latched before RXSETUP is cleared.
        if req_type & 0x80 != 0 {
            self.csr_set(0, udp::CSR_DIR);
        }
        self.csr_clear(0, udp::CSR_RXSETUP);

        match request {
            REQ_GET_STATUS => {
                let device = req_type & 0x1F == 0;
                self.ep0_reply(if device { &STATUS_SELF_POWERED } else { &STATUS_ZERO }, wlength);
            }
            REQ_CLEAR_FEATURE | REQ_SET_FEATURE | REQ_SET_INTERFACE => self.ep0_ack(),
            REQ_SET_ADDRESS => {
                // Takes effect only after the ack has gone out.
                self.pending_addr = Some((value & 0x7F) as u8);
                self.ep0_ack();
            }
            REQ_GET_DESCRIPTOR => match (value >> 8) as u8 {
                DESC_DEVICE => self.ep0_reply(&DEVICE_DESC, wlength),
                DESC_CONFIG => self.ep0_reply(&CONFIG_DESC, wlength),
                _ => self.ep0_stall(), // incl. device qualifier: full-speed only
            },
            REQ_GET_CONFIGURATION => {
                self.ep0_reply(if self.configured { &BYTE_ONE } else { &BYTE_ZERO }, wlength)
            }
            REQ_SET_CONFIGURATION => self.set_configuration(value as u8),
            REQ_GET_INTERFACE => self.ep0_reply(&BYTE_ZERO, wlength),
            _ => self.ep0_stall(),
        }
    }

    fn set_configuration(&mut self, config: u8) {
        if config > 1 {
            self.ep0_stall();
            return;
        }
        self.ep0_ack();
        let r = self.udp.regs();
        self.configured = config == 1;
        if self.configured {
            r.glbstate.set(udp::GLB_CONFG | udp::GLB_FADDEN);
            self.csr_write(EP_OUT, udp::CSR_EPEDS | udp::CSR_EPTYPE_BULK_OUT);
            self.csr_write(EP_IN, udp::CSR_EPEDS | udp::CSR_EPTYPE_BULK_IN);
            self.rx_bank1 = false;
        } else {
            r.glbstate.set(udp::GLB_FADDEN);
            self.csr_write(EP_OUT, 0);
            self.csr_write(EP_IN, 0);
        }
    }

    /// Reads one bulk OUT packet into `buf`; `None` if nothing is waiting.
    /// A packet longer than `buf` is truncated (the rest is discarded).
    pub fn read(&mut self, buf: &mut [u8; BULK_SIZE]) -> Option<usize> {
        if !self.configured {
            return None;
        }
        let r = self.udp.regs();
        let bank = if self.rx_bank1 { udp::CSR_RX_DATA_BK1 } else { udp::CSR_RX_DATA_BK0 };
        let csr = r.csr[EP_OUT].get();
        if csr & bank == 0 {
            return None;
        }
        let total = ((csr & udp::CSR_RXBYTECNT_MASK) >> udp::CSR_RXBYTECNT_SHIFT) as usize;
        let mut kept = 0;
        for i in 0..total {
            let b = r.fdr[EP_OUT].get() as u8;
            if i < buf.len() {
                buf[i] = b;
                kept = i + 1;
            }
        }
        self.csr_clear(EP_OUT, bank);
        self.rx_bank1 = !self.rx_bank1;
        Some(kept)
    }

    /// Queues one bulk IN packet (`data.len() <= 64`). Returns false if the
    /// previous packet hasn't been sent yet, the host hasn't configured us,
    /// or `data` is too long -- the caller retries on false.
    pub fn write(&mut self, data: &[u8]) -> bool {
        if !self.configured || data.len() > BULK_SIZE {
            return false;
        }
        let r = self.udp.regs();
        if r.csr[EP_IN].get() & udp::CSR_TXPKTRDY != 0 {
            return false;
        }
        for &b in data {
            r.fdr[EP_IN].set(b as u32);
        }
        self.csr_set(EP_IN, udp::CSR_TXPKTRDY);
        true
    }

    /// True while a queued bulk IN packet hasn't finished sending.
    pub fn tx_busy(&self) -> bool {
        self.udp.regs().csr[EP_IN].get() & udp::CSR_TXPKTRDY != 0
    }
}

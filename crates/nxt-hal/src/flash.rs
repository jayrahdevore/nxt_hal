//! Embedded flash controller: page programming and region locking.
//!
//! Flow follows the datasheet / NxOS `_efc.c`: the page's 64 words are
//! written (32-bit accesses only) into the flash address window, which
//! fills the controller's latch buffer, then a `WRITE_PAGE` command with
//! the page number programs it (the controller erases the page itself).
//!
//! Programming stalls bus access for milliseconds; callers that must
//! keep the AVR link alive have to re-service it afterwards.

use nxt_pac::mc::{self, Mc};
use nxt_pac::pmc::ClocksReady;

/// First flash address (`AT91C_IFLASH`).
const FLASH_BASE: usize = 0x0010_0000;
pub const PAGE_SIZE: usize = 256;
pub const PAGES: u16 = 1024;
pub const LOCK_REGIONS: u8 = 16;
const PAGES_PER_REGION: u16 = PAGES / LOCK_REGIONS as u16;

/// Upper bound on FRDY polling, ~100x a worst-case page program.
const SPIN_LIMIT: u32 = 3_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    BadPage,
    /// Page lies in a locked region.
    Locked,
    /// The controller reported a programming error.
    Program,
    Timeout,
}

pub struct Flash {
    mc: Mc,
}

impl Flash {
    /// `_clocks` proves `init_clocks` has programmed FMCN/FWS for 48 MHz.
    pub fn new(mc: Mc, _clocks: &ClocksReady) -> Self {
        Self { mc }
    }

    fn wait_ready(&self) -> Result<u32, Error> {
        for _ in 0..SPIN_LIMIT {
            // One FSR read per iteration; LOCKE/PROGE clear on read, so the
            // final read (with FRDY) must be the one that is checked.
            let fsr = self.mc.regs().fsr.get();
            if fsr & mc::FSR_FRDY != 0 {
                return Ok(fsr);
            }
        }
        Err(Error::Timeout)
    }

    /// Issues an EFC command (optionally latching a page of data first)
    /// via [`ram_command`] and decodes the status.
    fn command(
        &self,
        cmd: u32,
        page: u16,
        latch: Option<&[u32; PAGE_SIZE / 4]>,
    ) -> Result<(), Error> {
        self.wait_ready()?;
        let fcr = mc::FCR_KEY | ((page as u32) << mc::FCR_PAGEN_SHIFT) | cmd;
        let dst = (FLASH_BASE + page as usize * PAGE_SIZE) as *mut u32;
        let (src, words) = latch.map_or(
            (
                core::ptr::NonNull::<u32>::dangling().as_ptr() as *const u32,
                0,
            ),
            |w| (w.as_ptr(), w.len()),
        );
        let fsr = unsafe { ram_command(dst, src, words, fcr) };
        if fsr == 0 {
            Err(Error::Timeout)
        } else if fsr & mc::FSR_LOCKE != 0 {
            Err(Error::Locked)
        } else if fsr & mc::FSR_PROGE != 0 {
            Err(Error::Program)
        } else {
            Ok(())
        }
    }

    /// Programs `data` into flash page `page` (absolute page number).
    pub fn write_page(&mut self, page: u16, data: &[u8; PAGE_SIZE]) -> Result<(), Error> {
        if page >= PAGES {
            return Err(Error::BadPage);
        }
        let mut words = [0u32; PAGE_SIZE / 4];
        for (w, b) in words.iter_mut().zip(data.chunks_exact(4)) {
            *w = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        }
        self.command(mc::cmd::WRITE_PAGE, page, Some(&words))
    }

    /// Reads flash page `page` into `out`.
    pub fn read_page(&self, page: u16, out: &mut [u8; PAGE_SIZE]) {
        let src = (FLASH_BASE + page as usize * PAGE_SIZE) as *const u32;
        for (i, w) in out.chunks_exact_mut(4).enumerate() {
            w.copy_from_slice(&unsafe { src.add(i).read_volatile() }.to_le_bytes());
        }
    }

    pub fn is_region_locked(&self, region: u8) -> bool {
        self.mc.regs().fsr.get() & (1 << (mc::FSR_LOCKS_SHIFT + region as u32)) != 0
    }

    /// Sets the (non-volatile) lock bit of `region`; a locked region
    /// rejects programming until [`Self::unlock_region`] -- the only
    /// protection that survives a buggy bootloader.
    ///
    /// Passes the region's first page as the page number. Whether the
    /// controller decodes the region from PAGEN[9:6] (as the 64-page
    /// regions imply) rather than the header's "bits 4 to 7" wording is
    /// unverified: check `is_region_locked` afterwards on hardware.
    pub fn lock_region(&mut self, region: u8) -> Result<(), Error> {
        self.command(mc::cmd::SET_LOCK, region as u16 * PAGES_PER_REGION, None)
    }

    pub fn unlock_region(&mut self, region: u8) -> Result<(), Error> {
        self.command(mc::cmd::CLEAR_LOCK, region as u16 * PAGES_PER_REGION, None)
    }
}

/// Latches `words` words from `src` (none for lock commands) into the flash window at
/// `dst`, issues `fcr`, and spins until FRDY. Returns the FSR that showed
/// FRDY (so LOCKE/PROGE are intact), or 0 on timeout.
///
/// Runs from RAM (`.ramfunc`, copied there at startup like `.data`): while
/// the controller programs, flash can't serve instruction fetches, and
/// code executing from it would stall -- or, if the command never
/// completes, hang forever past any timeout. Must not call anything
/// (a call would land back in flash), so it is raw volatile accesses and
/// plain loops only; check the disassembly after editing.
#[link_section = ".ramfunc"]
#[inline(never)]
unsafe extern "C" fn ram_command(dst: *mut u32, src: *const u32, words: usize, fcr: u32) -> u32 {
    const MC_FCR: *mut u32 = 0xFFFF_FF64 as *mut u32;
    const MC_FSR: *const u32 = 0xFFFF_FF68 as *const u32;
    // Explicit count rather than a null check: a null `src` with a volatile
    // read is UB, so the optimiser deletes the check.
    let mut i = 0;
    while i < words {
        dst.add(i).write_volatile(src.add(i).read_volatile());
        i += 1;
    }
    MC_FCR.write_volatile(fcr);
    let mut n = 0u32;
    while n < SPIN_LIMIT {
        let s = MC_FSR.read_volatile();
        if s & mc::FSR_FRDY != 0 {
            return s;
        }
        n += 1;
    }
    0
}

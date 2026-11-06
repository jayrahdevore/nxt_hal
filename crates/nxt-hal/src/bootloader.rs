//! Re-entering the USB bootloader from a running app.
//!
//! Only meaningful for images linked behind the bootloader
//! (`link-app.x`); an image linked at the start of flash *is* the
//! firmware, and jumping to `FLASH_BASE` would just restart itself.

use nxt_boot_proto::{APP_BASE, BOOT_REQUEST_ADDR, BOOT_REQUEST_MAGIC, FLASH_BASE};
use nxt_pac::pmc::Pmc;

/// True if this code is running from the app region, i.e. a bootloader
/// exists below it.
pub fn available() -> bool {
    let here: fn() -> bool = available;
    (here as usize as u32) >= APP_BASE
}

/// Asks the bootloader for upload mode and restarts into it. Returns
/// (doing nothing) if no bootloader is below this image.
pub fn enter(pmc: &mut Pmc) {
    if !available() {
        return;
    }
    unsafe { (BOOT_REQUEST_ADDR as *mut u32).write_volatile(BOOT_REQUEST_MAGIC) };
    // Same hand-off the bootloader uses for the app: MCK back to the slow
    // clock so the next `init_clocks` doesn't rewrite the PLL under itself.
    pmc.slow_clock();
    unsafe { core::arch::asm!("bx {0}", in(reg) FLASH_BASE, options(noreturn)) }
}

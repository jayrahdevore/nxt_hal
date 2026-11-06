//! Boot/startup runtime for the NXT's AT91SAM7S256 (ARM7TDMI /
//! `armv4t-none-eabi`).
//!
//! Firmware binaries depend on this crate purely for its side effects
//! (the vector table and reset handler it places in `.vectors`/`.text`,
//! kept alive by `link.x`'s `KEEP()` and `ENTRY(_reset)`) and must supply
//! a `#[no_mangle] pub extern "C" fn main() -> !` for [`start::_start_rust`]
//! to call once `.data`/`.bss` are initialized.
#![no_std]

mod start;
mod vectors;

/// No display/UART is wired up yet (Milestone 0/1), so a panic has
/// nowhere to report to. Disable interrupts and spin, so it's at least
/// distinguishable from a hang under a debugger rather than silently
/// resetting or running off into garbage.
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}

/// Transfers control to the ARM-state image whose first instruction is at
/// `entry`, never returning. IRQ/FIQ stay masked (nothing enables them yet)
/// and the target's own reset code sets all banked stacks, so no state
/// needs to be carried over.
///
/// # Safety
/// `entry` must be the address of valid ARM code that never expects to
/// return, and the caller must have quiesced any peripheral the target
/// would reinitialise unsafely.
pub unsafe fn jump_to(entry: u32) -> ! {
    core::arch::asm!("bx {0}", in(reg) entry, options(noreturn))
}

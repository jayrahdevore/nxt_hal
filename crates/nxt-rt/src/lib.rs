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

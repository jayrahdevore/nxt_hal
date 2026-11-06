//! Plain-Rust half of the reset path: `.data`/`.bss` initialization, then
//! handoff to the firmware's `main`.
//!
//! The firmware binary provides `main` as a free `#[no_mangle] pub extern
//! "C" fn main() -> !` (the plain-function convention the `gba` crate
//! uses, rather than inventing a `#[entry]` proc-macro for Phase 1).

use core::ptr::{self, addr_of, addr_of_mut};

#[no_mangle]
pub unsafe extern "C" fn _start_rust() -> ! {
    extern "C" {
        static mut _sdata: u32;
        static mut _edata: u32;
        static _sidata: u32;
        static mut _sbss: u32;
        static mut _ebss: u32;

        fn main() -> !;
    }

    let data_start = addr_of_mut!(_sdata);
    let data_end = addr_of_mut!(_edata);
    let data_words = data_end.offset_from(data_start) as usize;
    ptr::copy_nonoverlapping(addr_of!(_sidata), data_start, data_words);

    let bss_start = addr_of_mut!(_sbss);
    let bss_end = addr_of_mut!(_ebss);
    let bss_words = bss_end.offset_from(bss_start) as usize;
    ptr::write_bytes(bss_start, 0, bss_words);

    main()
}

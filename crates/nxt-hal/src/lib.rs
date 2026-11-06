//! Safe, typestate-constrained HAL for the LEGO NXT, built on [`nxt_pac`].
#![no_std]

pub mod avr;
pub mod bootloader;
pub mod flash;
pub mod gpio;
pub mod lcd;
pub mod sound;
pub mod usb;

pub use nxt_pac::pit::Pit;
pub use nxt_pac::Peripherals;
pub use nxt_pac::pmc::Pmc;

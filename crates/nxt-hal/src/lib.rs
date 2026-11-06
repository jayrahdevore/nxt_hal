//! Safe, typestate-constrained HAL for the LEGO NXT, built on [`nxt_pac`].
#![no_std]

pub mod avr;
pub mod gpio;
pub mod sound;

pub use nxt_pac::pit::Pit;
pub use nxt_pac::Peripherals;

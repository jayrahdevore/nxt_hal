//! Safe GPIO API over PIOA, built on [`nxt_pac::pio::Pio`].
//!
//! For now a [`Pin`] takes ownership of the *entire* PIOA peripheral
//! rather than splitting it into 32 independently-owned lines --
//! Milestone 0 only ever drives one pin at a time, and a `split()`-style
//! API is deferred until a milestone actually needs more than one line
//! live at once.

use core::convert::Infallible;
use core::marker::PhantomData;
use embedded_hal::digital::{ErrorType, OutputPin};
use nxt_pac::pio::{ClockEnabled, Pio};

/// Marker type for a pin configured as a push-pull output.
pub struct Output;

/// A single PIOA line (`PA{N}`), typestated by its configured mode.
pub struct Pin<const N: u8, MODE> {
    pio: Pio,
    _mode: PhantomData<MODE>,
}

impl<const N: u8> Pin<N, Output> {
    /// Claims `PA{N}` for PIO control and configures it as an output.
    /// `clock` is the proof token from
    /// [`nxt_pac::pmc::Pmc::enable_pioa_clock`] that PIOA's clock is
    /// enabled -- firmware doesn't need to touch `nxt_pac::pio::Pio`
    /// directly, it's fully wrapped here.
    pub fn new_output(clock: ClockEnabled) -> Self {
        let mut pio = Pio::new(clock);
        pio.enable_output(1 << N);
        Self {
            pio,
            _mode: PhantomData,
        }
    }
}

impl<const N: u8, MODE> ErrorType for Pin<N, MODE> {
    type Error = Infallible;
}

impl<const N: u8> OutputPin for Pin<N, Output> {
    fn set_low(&mut self) -> Result<(), Self::Error> {
        self.pio.set_low(1 << N);
        Ok(())
    }

    fn set_high(&mut self) -> Result<(), Self::Error> {
        self.pio.set_high(1 << N);
        Ok(())
    }
}

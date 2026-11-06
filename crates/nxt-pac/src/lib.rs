//! Register-level peripheral access for the LEGO NXT's AT91SAM7S256.
//!
//! There is no SVD for this chip, so these definitions are hand-written
//! from Atmel's own generated header (`AT91SAM7S256.h`, as shipped in the
//! `at91lib` BSP) and cross-checked against the datasheet. Register names
//! follow Atmel's naming (e.g. `WDTC_WDMR`, not `WDT_MR`) so they can be
//! traced back to the reference material.
//!
//! This crate only maps "what the datasheet says the bits do" — no policy,
//! no safety wrappers beyond what's needed to take ownership of a
//! peripheral exactly once. See `nxt-hal` for the safe, typestate-based API
//! built on top of this.
#![no_std]

pub mod mc;
pub mod pio;
pub mod pit;
pub mod pmc;
pub mod ssc;
pub mod twi;
pub mod udp;
pub mod wdt;

// armv4t-none-eabi has `max_atomic_width = 0` (ARMv4T predates
// LDREX/STREX) -- no `core::sync::atomic` types are available at all, so
// this can't use `AtomicBool`. A plain `static mut` is fine here: nothing
// runs concurrently with `main` yet (IRQ/FIQ stay masked through
// Milestone 0/1), so there's no real race to guard against, just the
// "called twice by mistake" footgun.
static mut TAKEN: bool = false;

/// All peripherals that don't require a clock-enable proof token up
/// front. `pio::Pio` is deliberately not a field here — it's constructed
/// separately once `pmc.enable_pioa_clock()` has produced the proof it
/// requires.
pub struct Peripherals {
    pub pmc: pmc::Pmc,
    pub mc: mc::Mc,
    pub wdt: wdt::Wdt,
}

impl Peripherals {
    /// Takes ownership of the device's peripherals exactly once. Returns
    /// `None` on any subsequent call, so two `&mut` handles to the same
    /// memory-mapped registers can never coexist.
    pub fn take() -> Option<Self> {
        unsafe {
            if TAKEN {
                None
            } else {
                TAKEN = true;
                Some(Self::steal())
            }
        }
    }

    /// # Safety
    /// Caller must ensure no other live `Peripherals` exists.
    pub unsafe fn steal() -> Self {
        Self {
            pmc: pmc::Pmc::new(),
            mc: mc::Mc::new(),
            wdt: wdt::Wdt::new(),
        }
    }
}

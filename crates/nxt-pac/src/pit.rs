//! Periodic Interval Timer, polled (no interrupt). `AT91C_BASE_PITC =
//! 0xFFFFFD30`, register order and bits verbatim from NxOS's header.
//! Counts MCK/16.

use vcell::VolatileCell;

#[repr(C)]
pub struct RegisterBlock {
    pub pimr: VolatileCell<u32>, // 0x00
    pub pisr: VolatileCell<u32>, // 0x04
    pub pivr: VolatileCell<u32>, // 0x08 (reading clears PITS)
    pub piir: VolatileCell<u32>, // 0x0C
}

const PITEN: u32 = 1 << 24;
const PITS: u32 = 1;

pub struct Pit {
    _private: (),
}

impl Pit {
    const PTR: *mut RegisterBlock = 0xFFFF_FD30 as *mut RegisterBlock;

    /// Starts a 1 ms tick; assumes MCK = 48 MHz (see `Pmc::init_clocks`).
    pub fn new_1ms() -> Self {
        let pit = Self { _private: () };
        pit.regs().pimr.set((48_000_000 / 16 / 1000 - 1) | PITEN);
        pit
    }

    fn regs(&self) -> &RegisterBlock {
        unsafe { &*Self::PTR }
    }

    /// Free-running 12-bit count of 1 ms ticks (PICNT, read from PIIR so it
    /// does not disturb the status flag). Wraps at 4096.
    pub fn ticks(&self) -> u32 {
        self.regs().piir.get() >> 20
    }

    /// Blocks until the next 1 ms tick boundary. Gives up after a spin
    /// limit (returns `false`) so a misconfigured timer can't hang the firmware.
    pub fn wait_tick(&mut self) -> bool {
        let mut spins = 0u32;
        while self.regs().pisr.get() & PITS == 0 {
            spins += 1;
            if spins > 2_000_000 {
                return false;
            }
        }
        let _ = self.regs().pivr.get();
        true
    }

    pub fn wait_ms(&mut self, ms: u32) -> bool {
        (0..ms).all(|_| self.wait_tick())
    }
}

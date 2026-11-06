//! ARM<->AVR co-processor link, polled. Protocol per
//! `docs/adr/0001-motor-avr-link.md` and NxOS `avr.c`.
//!
//! Until the handshake is sent the AVR makes the "clicking brick" noise
//! and eventually powers the brick down. After it, the AVR expects a
//! steady exchange: call [`AvrLink::service`] at least every ~1 ms. A gap
//! of several ms (observed: a blocking tone cycle) drops the link; it is
//! then re-established with a fresh handshake, as NxOS does after 3 bad
//! replies. A timer interrupt would be the robust long-term driver.

use nxt_pac::pio::Pio;
use nxt_pac::pit::Pit;
use nxt_pac::pmc::Pmc;
pub use nxt_pac::twi::Error;
use nxt_pac::twi::Twi;

const AVR_ADDRESS: u8 = 1;
const HANDSHAKE: &[u8] = b"\xCCLet's samba nxt arm in arm, (c)LEGO System A/S";
const MAX_FAILURES: u8 = 3;

/// A front-panel button, decoded from the AVR's single analog button level.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Button {
    None,
    /// Orange. Verified on hardware: reads ~2048 when held, 0..5 when idle.
    Enter,
    /// The three below are confirmed to read as "pressed" on hardware (each beeps), but which variant maps to which physical button is unverified; thresholds are from memory of NxOS.
    Cancel,
    Right,
    Left,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Next {
    Handshake,
    /// After the handshake NxOS waits ~3 ms before the first transfer.
    Settle(u32),
    Write,
    Read,
}

pub struct AvrLink {
    twi: Twi,
    next: Next,
    last_tick: u32,
    failures: u8,
    pub battery_mv: u32,
    /// Raw 10-bit button reading from the AVR reply (rx[8..10]); one analog
    /// level per button, decoded by [`AvrLink::button`].
    pub buttons_raw: u16,
    /// Last checksum-valid reply, for debugging the packet layout.
    pub last_rx: [u8; 13],
    /// Debug counters: completed exchanges, failed transactions, re-handshakes.
    pub ok_count: u32,
    pub fail_count: u32,
    pub rehandshakes: u32,
}

impl AvrLink {
    /// Brings up the TWI bus. The first handshake happens on the first
    /// [`service`](Self::service) call.
    pub fn new(pmc: &mut Pmc) -> Self {
        let twi_clock = pmc.enable_twi_clock();
        let mut pio = Pio::new(pmc.enable_pioa_clock());
        let twi = Twi::new(twi_clock, &mut pio);
        Self {
            twi,
            next: Next::Handshake,
            last_tick: u32::MAX,
            failures: 0,
            battery_mv: 0,
            buttons_raw: 0,
            last_rx: [0; 13],
            ok_count: 0,
            fail_count: 0,
            rehandshakes: 0,
        }
    }

    /// The button currently held, from the last valid AVR reply.
    pub fn button(&self) -> Button {
        match self.buttons_raw {
            1024.. => Button::Enter,
            721..=1023 => Button::Cancel,
            271..=720 => Button::Right,
            61..=270 => Button::Left,
            _ => Button::None,
        }
    }

    /// Advances the link by at most one short bus transaction, and only
    /// once per PIT tick (1 ms), so callers can poll it freely from busy
    /// loops. Never blocks for more than one transaction (~0.3 ms).
    pub fn service(&mut self, pit: &Pit) {
        let now = pit.ticks();
        if now == self.last_tick {
            return;
        }
        self.last_tick = now;

        let result = match self.next {
            Next::Handshake => {
                let r = self.twi.write(AVR_ADDRESS, HANDSHAKE);
                if r.is_ok() {
                    self.next = Next::Settle(now.wrapping_add(3) & 0xFFF);
                }
                r
            }
            Next::Settle(until) => {
                if now == until {
                    self.next = Next::Write;
                }
                Ok(())
            }
            Next::Write => {
                let mut tx = [0u8; 9];
                tx[1] = 8; // PWM frequency, normal run mode (motors off)
                tx[8] = !tx[..8].iter().fold(0u8, |a, b| a.wrapping_add(*b));
                let r = self.twi.write(AVR_ADDRESS, &tx);
                if r.is_ok() {
                    self.next = Next::Read;
                }
                r
            }
            Next::Read => {
                let mut rx = [0u8; 13];
                let r = self.twi.read(AVR_ADDRESS, &mut rx);
                if r.is_ok() {
                    self.next = Next::Write;
                    if rx.iter().fold(0u8, |a, b| a.wrapping_add(*b)) == 0xFF {
                        let word = u16::from_le_bytes([rx[10], rx[11]]) as u32;
                        self.last_rx = rx;
                        self.buttons_raw = u16::from_le_bytes([rx[8], rx[9]]);
                        self.battery_mv = ((word & 0x3ff) * 3545) >> 9;
                        self.ok_count += 1;
                        self.failures = 0;
                    } else {
                        // Bad checksum counts as a failure (NxOS: 3 in a row
                        // restarts the link).
                        self.failures += 1;
                    }
                }
                r
            }
        };

        if result.is_err() {
            self.fail_count += 1;
            self.failures += 1;
        }
        if self.failures >= MAX_FAILURES {
            self.failures = 0;
            self.rehandshakes += 1;
            self.next = Next::Handshake;
        }
    }
}

//! Spins motor A while the left button is held, brakes when released.
//!
//! Build as a bootloader app (`just app-build nxt-motor`) and upload with
//! `just app-upload nxt-motor`. Holding Orange for a second re-enters the
//! USB bootloader so the app can be replaced again.
#![no_std]
#![no_main]

use nxt_hal::avr::{AvrLink, Button, Hold, Motor};
use nxt_hal::bootloader;
use nxt_hal::{Peripherals, Pit};

// Pulled in only for its side effects: the vector table and reset handler.
extern crate nxt_rt as _;

const MOTOR_POWER: i8 = 75;

#[no_mangle]
pub extern "C" fn main() -> ! {
    let peripherals = Peripherals::take().expect("Peripherals::take must only be called once");
    let mut pmc = peripherals.pmc;
    let mut wdt = peripherals.wdt;

    wdt.disable();
    pmc.init_clocks();

    let pit = Pit::new_1ms();
    let mut avr = AvrLink::new(&mut pmc);

    // Holding Left+Right this long restarts into the USB bootloader (only when
    // linked behind it; see `bootloader::available`).
    const FLASH_MODE_HOLD_MS: u32 = 1000;
    let mut chord = Hold::default();
    let mut last_tick = pit.ticks();
    loop {
        avr.service(&pit);
        let t = pit.ticks();
        if t != last_tick {
            last_tick = t;
            let button = avr.button();
            if button == Button::Left {
                avr.set_motor(Motor::A, MOTOR_POWER, false);
            } else {
                avr.set_motor(Motor::A, 0, true);
            }
            let held_ms = chord.update(button == Button::LeftRight);
            if held_ms >= FLASH_MODE_HOLD_MS && bootloader::available() {
                avr.set_motor(Motor::A, 0, true);
                bootloader::enter(&mut pmc);
            }
        }
    }
}

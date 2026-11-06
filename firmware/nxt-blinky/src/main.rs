//! Milestone 0: bring up clocks, the AVR link and sound, beep on any button
//! press, and toggle one
//! PIOA line forever.
//!
//! This is the first thing to flash onto real hardware -- it exercises
//! the vector table, reset handler, linker script, `build-std=core`,
//! `objcopy`, and `fwflash` end to end without touching the still-
//! unresolved motor/AVR-link question (see `docs/adr/0001-motor-avr-link.md`).
//!
//! Toggles PA23, which is Sensor Port 1's DIGI0 line -- confirmed against
//! the nxos project's `sensors.c` pin map (`sensors_pinmap[0] = {
//! AT91C_PIO_PA23, AT91C_PIO_PA18 }`) as a line nothing else on the board
//! contends for, so it's safe to drive as a plain GPIO output. Clip a
//! multimeter or an LED+resistor between Sensor Port 1's DIGI0 pin and
//! ground to see it toggle.
#![no_std]
#![no_main]

use embedded_hal::digital::OutputPin;
use nxt_hal::avr::{AvrLink, Button};
use nxt_hal::bootloader;
use nxt_hal::gpio::{Output, Pin};
use nxt_hal::sound::Speaker;
use nxt_hal::{Peripherals, Pit};

// Pulled in only for its side effects: the vector table and reset handler
// it places in `.vectors`/`.text`.
extern crate nxt_rt as _;

const SENSOR_PORT_1_DIGI0: u8 = 23; // PA23

#[no_mangle]
pub extern "C" fn main() -> ! {
    let peripherals = Peripherals::take().expect("Peripherals::take must only be called once");
    let mut pmc = peripherals.pmc;
    let mut wdt = peripherals.wdt;

    wdt.disable();
    pmc.init_clocks();
    let ssc_clock = pmc.enable_ssc_clock();
    let mut speaker = Speaker::new(ssc_clock, pmc.enable_pioa_clock());

    // The AVR must keep hearing from us or it makes the "clicking brick"
    // noise and eventually powers the brick down. `service` is cheap to
    // poll and does one short bus transaction per millisecond.
    let pit = Pit::new_1ms();
    let mut avr = AvrLink::new(&mut pmc);

    // Startup jingle, servicing the AVR link while the tones play.
    for (freq, ms) in [(523, 150), (659, 150), (784, 250)] {
        speaker.beep(freq, ms, || avr.service(&pit));
    }

    let clock = pmc.enable_pioa_clock();
    let mut pin: Pin<SENSOR_PORT_1_DIGI0, Output> = Pin::new_output(clock);

    let mut was_pressed = false;
    // Holding Orange this long restarts into the USB bootloader (only when
    // linked behind it; see `bootloader::available`).
    const FLASH_MODE_HOLD_MS: u32 = 1000;
    let mut held_ms: u32 = 0;

    // Blink PA23 every 200 ms, keeping the AVR link alive in between.
    let mut last_tick = pit.ticks();
    let mut ms: u32 = 0;
    loop {
        avr.service(&pit);
        // Beep once per press (rising edge); the beep keeps the AVR serviced.
        let pressed = avr.button() != Button::None;
        if pressed && !was_pressed {
            speaker.beep(880, 100, || avr.service(&pit));
        }
        was_pressed = pressed;
        let t = pit.ticks();
        if t != last_tick {
            last_tick = t;
            ms += 1;
            held_ms = if pressed { held_ms + 1 } else { 0 };
            if held_ms >= FLASH_MODE_HOLD_MS && bootloader::available() {
                // Flash-mode tone (same rising pair the bootloader plays).
                for freq in [988, 1319] {
                    speaker.beep(freq, 100, || avr.service(&pit));
                }
                bootloader::enter(&mut pmc);
            }
            if ms % 200 == 0 {
                if (ms / 200) % 2 == 0 {
                    pin.set_low().unwrap();
                } else {
                    pin.set_high().unwrap();
                }
            }
        }
    }
}

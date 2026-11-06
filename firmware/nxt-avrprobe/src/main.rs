//! RAM-resident probe: upload to 0x00202000 with SAM-BA writes, run with
//! `G00202000#`. Replays nxt-blinky's startup (clocks, speaker, AVR link,
//! jingle) for well under a second, then *returns* to the SAM-BA monitor
//! so the mailbox can be read over USB (a long-running probe starves the
//! monitor's USB servicing and the host resets the device).
//!
//! Mailbox at 0x0020D000 (u32 words): [0]=stage, [1]=ok exchanges,
//! [2]=failed transactions, [3]=re-handshakes, [4]=battery mV, [5]=SSC SR,
//! [6]=SSC PTSR, [7]=SSC TFMR, [8]=PA17 owned by PIO?, [9]=PA17 edges and
//! [10]=PA17 high samples over 20k reads after the beep.
#![no_std]
#![no_main]

use nxt_hal::avr::AvrLink;
use nxt_hal::sound::Speaker;
use nxt_hal::{Peripherals, Pit};

const MAILBOX: *mut u32 = 0x0020_D000 as *mut u32;
fn mb(i: usize, v: u32) {
    unsafe { MAILBOX.add(i).write_volatile(v) }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    mb(0, 0xDEAD);
    loop {}
}

#[no_mangle]
#[link_section = ".text.probe"]
pub extern "C" fn probe() {
    for i in 0..16 {
        mb(i, 0);
    }
    mb(0, 1);
    let mut pmc = unsafe { Peripherals::steal() }.pmc;
    pmc.init_clocks();
    mb(0, 2);
    let ssc_clock = pmc.enable_ssc_clock();
    let mut speaker = Speaker::new(ssc_clock, pmc.enable_pioa_clock());
    let pit = Pit::new_1ms();
    let mut avr = AvrLink::new(&mut pmc);
    mb(0, 3);
    speaker.beep(523, 200, || avr.service(&pit));
    mb(0, 4);
    // Probe 1: stop the SSC transmitter and park PA17 low as a plain GPIO.
    unsafe {
        ((0xFFFD_4000u32) as *mut u32).write_volatile(1 << 9); // CR_TXDIS
        ((0xFFFF_F400u32) as *mut u32).write_volatile(1 << 17); // PER
        ((0xFFFF_F400u32 + 0x34) as *mut u32).write_volatile(1 << 17); // CODR
        ((0xFFFF_F400u32 + 0x10) as *mut u32).write_volatile(1 << 17); // OER
    }
    // ~700 ms of servicing; record the min/max raw button value seen and a
    // coarse histogram (16 bins of 64) so each held button shows up.
    let mut bins = [0u32; 16];
    let (mut lo, mut hi) = (u32::MAX, 0u32);
    let start = pit.ticks();
    while (pit.ticks().wrapping_sub(start) & 0xFFF) < 700 {
        avr.service(&pit);
        let b = avr.buttons_raw as u32;
        lo = lo.min(b);
        hi = hi.max(b);
        bins[((b >> 6) & 15) as usize] += 1;
    }
    for i in 0..16 {
        mb(16 + i, bins[i]);
    }
    for w in 0..4 {
        let b = avr.last_rx;
        let o = w * 4;
        let v = |i: usize| if i < 13 { b[i] as u32 } else { 0 };
        mb(
            32 + w,
            v(o) | v(o + 1) << 8 | v(o + 2) << 16 | v(o + 3) << 24,
        );
    }
    mb(11, lo);
    mb(12, hi);
    // Is TD (PA17) still moving after the jingle? Count transitions of the
    // pin level over ~20k reads while the SSC sits idle.
    let rd = |a: u32| unsafe { (a as *const u32).read_volatile() };
    const SSC: u32 = 0xFFFD_4000;
    const PIOA: u32 = 0xFFFF_F400;
    mb(5, rd(SSC + 0x40)); // SSC SR (bit1 TXEMPTY, bit0 TXRDY)
    mb(6, rd(SSC + 0x124)); // PDC PTSR
    mb(7, rd(SSC + 0x1C)); // TFMR
    mb(8, (rd(PIOA + 0x08) >> 17) & 1); // PSR: 1 = PIO owns PA17, 0 = peripheral
    let mut last = (rd(PIOA + 0x3C) >> 17) & 1;
    let mut edges = 0u32;
    let mut highs = 0u32;
    for _ in 0..20_000 {
        let v = (rd(PIOA + 0x3C) >> 17) & 1;
        edges += (v != last) as u32;
        highs += v;
        last = v;
    }
    mb(9, edges);
    mb(10, highs);
    mb(1, avr.ok_count);
    mb(2, avr.fail_count);
    mb(3, avr.rehandshakes);
    mb(4, avr.battery_mv);
    mb(0, 0xD0FE);
}

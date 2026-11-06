//! USB bootloader. Link with `link-boot.x` (see `just boot-build`).
//!
//! Boot decision: once the AVR link is up (the only way to read buttons),
//! Orange held continuously for [`HOLD_MS`] -- or no valid app installed --
//! enters upload mode; anything else runs the app. Upload mode enumerates
//! over USB and speaks the `nxt-boot-proto` command protocol.
//!
//! Everything here is polled, and the AVR link is serviced from every wait
//! loop. Flash programming stalls the CPU for milliseconds per page, so
//! the link may drop during an upload; `AvrLink::service` re-handshakes.
//!
//! Unverified on hardware.
#![no_std]
#![no_main]

use nxt_boot_proto::{
    crc32_update, encode_info, page_in_image_region, AppHeader, ParseError, Request, Status, APP_FIRST_PAGE, BOOT_SIZE,
    HEADER_SIZE, IMAGE_BASE, PACKET_SIZE, PAGE_SIZE,
};
use nxt_hal::avr::{AvrLink, Button};
use nxt_hal::flash::Flash;
use nxt_hal::lcd::Lcd;
use nxt_hal::sound::Speaker;
use nxt_hal::usb::Usb;
use nxt_hal::{Peripherals, Pit};

extern crate nxt_rt as _;

// Splash bitmap, generated from assets/wrench.svg by tools/img2lcd/convert.sh.
mod wrench;

/// Orange must stay down this long after the AVR link comes up.
const HOLD_MS: u32 = 1500;
/// Give up waiting for the first valid AVR reply after this long.
const LINK_TIMEOUT_MS: u32 = 1000;
/// Give up queueing a USB reply after this long (host went away).
const REPLY_TIMEOUT_MS: u32 = 500;

/// Flash page of the (boot-region) lock region 0.
#[cfg(feature = "lock-boot")]
const BOOT_REGION: u8 = 0;
const _: () = assert!(BOOT_SIZE == 16 * 1024);

/// Millisecond clock over the PIT's 12-bit tick counter; `step` returns
/// true once per elapsed millisecond.
struct Millis {
    last: u32,
}

impl Millis {
    fn new(pit: &Pit) -> Self {
        Self { last: pit.ticks() }
    }

    fn step(&mut self, pit: &Pit) -> bool {
        let t = pit.ticks();
        if t == self.last {
            return false;
        }
        self.last = t;
        true
    }
}

/// Reads and validates the installed app (header fields and image CRC).
/// The CRC is computed in slices with `background` called between them, so
/// a large image doesn't starve the AVR.
fn check_app(mut background: impl FnMut()) -> Option<AppHeader> {
    let mut raw = [0u8; HEADER_SIZE];
    for (i, b) in raw.iter_mut().enumerate() {
        *b = unsafe { ((nxt_boot_proto::APP_BASE as usize + i) as *const u8).read_volatile() };
    }
    let header = AppHeader::parse(&raw).ok()?;
    let crc = image_crc(header.length, &mut background);
    (crc == header.crc32).then_some(header)
}

fn image_crc(length: u32, background: &mut impl FnMut()) -> u32 {
    let mut crc = !0u32;
    let mut off = 0usize;
    while off < length as usize {
        let n = (length as usize - off).min(PAGE_SIZE);
        let chunk = unsafe { core::slice::from_raw_parts((IMAGE_BASE as usize + off) as *const u8, n) };
        crc = crc32_update(crc, chunk);
        off += n;
        background();
    }
    !crc
}

#[no_mangle]
pub extern "C" fn main() -> ! {
    let peripherals = Peripherals::take().expect("Peripherals::take must only be called once");
    let mut pmc = peripherals.pmc;
    let mut wdt = peripherals.wdt;

    // An app that wants flash mode leaves a magic word in RAM; consume it
    // so a later reset boots normally.
    let requested = unsafe {
        let p = nxt_boot_proto::BOOT_REQUEST_ADDR as *mut u32;
        let hit = p.read_volatile() == nxt_boot_proto::BOOT_REQUEST_MAGIC;
        p.write_volatile(0);
        hit
    };

    // The watchdog is on by default and write-once: disable it first.
    wdt.disable();
    let clocks = pmc.init_clocks();
    let mut flash = Flash::new(peripherals.mc, &clocks);

    #[cfg(feature = "lock-boot")]
    if !flash.is_region_locked(BOOT_REGION) {
        let _ = flash.lock_region(BOOT_REGION);
    }

    let mut speaker = Speaker::new(pmc.enable_ssc_clock(), pmc.enable_pioa_clock());
    let pit = Pit::new_1ms();
    let mut avr = AvrLink::new(&mut pmc);

    // "Bootloader is running" blip on every power-up, before any decision:
    // one short low tone. If you never hear it, the bootloader itself isn't
    // executing (e.g. the SAM-BA install did not take).
    speaker.beep(659, 100, || avr.service(&pit));

    // Splash screen. Drawn on every power-up, before the boot decision; an
    // app that doesn't use the LCD leaves it up. The ~40 ms of LCD reset
    // waits keep the AVR link serviced.
    let mut lcd = Lcd::new(pmc.enable_spi_clock(), pmc.enable_pioa_clock(), &pit, || avr.service(&pit));
    lcd.flush(&wrench::WRENCH);

    // Wait for the first valid AVR reply: before that `button()` reads 0,
    // which is indistinguishable from "nothing pressed".
    let mut clock = Millis::new(&pit);
    let mut waited = 0;
    while avr.ok_count == 0 && waited < LINK_TIMEOUT_MS {
        avr.service(&pit);
        if clock.step(&pit) {
            waited += 1;
        }
    }
    let link_up = avr.ok_count > 0;

    let app = check_app(|| avr.service(&pit));

    // Why-we-are-here tones, played before the decision (see ADR 0003):
    //   very low tone  = AVR link never came up (button reads are blind)
    //   falling pair   = no valid app installed (header/CRC check failed)
    if !link_up {
        speaker.beep(262, 100, || avr.service(&pit));
    }
    if app.is_none() {
        for freq in [784, 523] {
            speaker.beep(freq, 100, || avr.service(&pit));
        }
    }

    let mut upload = app.is_none() || requested;
    if link_up && !upload {
        let mut held = 0;
        while held < HOLD_MS {
            avr.service(&pit);
            if avr.button() != Button::Enter {
                break;
            }
            if clock.step(&pit) {
                held += 1;
            }
        }
        upload = held >= HOLD_MS;
    }

    if !upload {
        // `app` is Some here.
        launch(&mut pmc, None, app.unwrap().entry);
    }

    // Flash-mode tone: a rising pair, distinct from the app's startup jingle.
    for freq in [988, 1319] {
        speaker.beep(freq, 100, || avr.service(&pit));
    }
    let mut usb = Usb::new(&mut pmc);
    usb.connect();
    upload_loop(&mut pmc, &mut usb, &mut flash, &mut avr, &pit, &mut speaker, app)
}

/// Quiesces what the app will reinitialise, then jumps.
fn launch(pmc: &mut nxt_hal::Pmc, usb: Option<&mut Usb>, entry: u32) -> ! {
    if let Some(usb) = usb {
        usb.disconnect();
    }
    pmc.slow_clock();
    unsafe { nxt_rt::jump_to(entry) }
}

enum Rx {
    Command,
    /// Collecting a page's data; `page` is `None` when the WRITE was
    /// rejected and the data is being discarded.
    PageData { page: Option<u16>, got: usize, status: Status },
}

fn upload_loop(
    pmc: &mut nxt_hal::Pmc,
    usb: &mut Usb,
    flash: &mut Flash,
    avr: &mut AvrLink,
    pit: &Pit,
    speaker: &mut Speaker,
    mut installed: Option<AppHeader>,
) -> ! {
    let mut rx = Rx::Command;
    let mut begun = false;
    let mut page_buf = [0u8; PAGE_SIZE];
    let mut pkt = [0u8; PACKET_SIZE];
    let mut enumerated = false;

    loop {
        avr.service(pit);
        usb.poll();
        // One tick when the host finishes enumerating: tells "USB never came
        // up" apart from "never entered flash mode".
        if !enumerated && usb.configured() {
            enumerated = true;
            speaker.beep(2093, 100, || {
                avr.service(pit);
                usb.poll();
            });
        }
        let Some(n) = usb.read(&mut pkt) else { continue };

        match rx {
            Rx::PageData { page, ref mut got, status } => {
                let take = n.min(PAGE_SIZE - *got);
                page_buf[*got..*got + take].copy_from_slice(&pkt[..take]);
                *got += take;
                if *got == PAGE_SIZE {
                    let result = match page {
                        Some(p) => match flash.write_page(p, &page_buf) {
                            Ok(()) => Status::Ok,
                            Err(_) => Status::FlashError,
                        },
                        None => status,
                    };
                    rx = Rx::Command;
                    reply(usb, avr, pit, &[result as u8]);
                }
            }
            Rx::Command => match Request::parse(&pkt[..n]) {
                Ok(Request::Info) => reply(usb, avr, pit, &encode_info(installed)),
                Ok(Request::Begin) => {
                    // Erase the header page first, so a half-written image
                    // can never validate.
                    installed = None;
                    let status = match flash.write_page(APP_FIRST_PAGE, &[0xFF; PAGE_SIZE]) {
                        Ok(()) => {
                            begun = true;
                            Status::Ok
                        }
                        Err(_) => Status::FlashError,
                    };
                    reply(usb, avr, pit, &[status as u8]);
                }
                Ok(Request::Write { page }) => {
                    let (page, status) = if !begun {
                        (None, Status::NotBegun)
                    } else if !page_in_image_region(page) {
                        (None, Status::BadPage)
                    } else {
                        (Some(page), Status::Ok)
                    };
                    rx = Rx::PageData { page, got: 0, status };
                }
                Ok(Request::Finish { header }) => {
                    let status = if !begun {
                        Status::NotBegun
                    } else if image_crc(header.length, &mut || avr.service(pit)) != header.crc32 {
                        Status::BadCrc
                    } else {
                        let mut page = [0xFFu8; PAGE_SIZE];
                        page[..HEADER_SIZE].copy_from_slice(&header.to_bytes());
                        match flash.write_page(APP_FIRST_PAGE, &page) {
                            Ok(()) => {
                                begun = false;
                                installed = Some(header);
                                Status::Ok
                            }
                            Err(_) => Status::FlashError,
                        }
                    };
                    reply(usb, avr, pit, &[status as u8]);
                    if status == Status::Ok {
                        // Upload done: rising triple.
                        for freq in [1319, 1568, 2093] {
                            speaker.beep(freq, 100, || {
                                avr.service(pit);
                                usb.poll();
                            });
                        }
                    }
                }
                Ok(Request::Boot) => match check_app(|| avr.service(pit)) {
                    Some(h) => {
                        reply(usb, avr, pit, &[Status::Ok as u8]);
                        // Let the host see the reply before the pull-up drops.
                        let mut clock = Millis::new(pit);
                        let mut ms = 0;
                        while ms < 20 {
                            avr.service(pit);
                            usb.poll();
                            if clock.step(pit) {
                                ms += 1;
                            }
                        }
                        launch(pmc, Some(usb), h.entry);
                    }
                    None => reply(usb, avr, pit, &[Status::NoApp as u8]),
                },
                Err(ParseError::BadHeader(_)) => reply(usb, avr, pit, &[Status::BadHeader as u8]),
                Err(_) => reply(usb, avr, pit, &[Status::BadCommand as u8]),
            },
        }
    }
}

/// Queues one reply packet, servicing the AVR and USB until it fits (or
/// the host has clearly gone away).
fn reply(usb: &mut Usb, avr: &mut AvrLink, pit: &Pit, data: &[u8]) {
    let mut clock = Millis::new(pit);
    let mut waited = 0;
    while !usb.write(data) {
        avr.service(pit);
        usb.poll();
        if clock.step(pit) {
            waited += 1;
            if waited >= REPLY_TIMEOUT_MS {
                return;
            }
        }
    }
}

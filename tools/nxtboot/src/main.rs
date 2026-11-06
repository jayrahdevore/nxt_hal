//! Host uploader for the `nxt-boot` USB bootloader.
//!
//! ```text
//! nxtboot info                     query the bootloader
//! nxtboot flash [--no-boot] APP.bin
//!                                  upload an app image (linked with ld/link-app.x),
//!                                  then boot it unless --no-boot
//! nxtboot verify APP.bin           compare the installed image with a local file
//! nxtboot boot                     validate + run the installed app
//! nxtboot pack BOOT.bin APP.bin OUT.bin
//!                                  build a full 256 KiB image for the one-time
//!                                  `fwflash` install (bootloader + app)
//! ```
use std::process::ExitCode;
use std::time::Duration;

use nxt_boot_proto::*;
use rusb::{DeviceHandle, GlobalContext};

const EP_OUT: u8 = 0x01;
const EP_IN: u8 = 0x82;
const TIMEOUT: Duration = Duration::from_secs(5);

/// Pads to a whole word with erased-flash 0xFF. The padded length goes in
/// the header and CRC, so the device's read-back check matches.
fn word_padded(app: &[u8]) -> Vec<u8> {
    let mut v = app.to_vec();
    while v.len() % 4 != 0 {
        v.push(0xFF);
    }
    v
}

fn header_for(app: &[u8]) -> Result<AppHeader, String> {
    if app.is_empty() || app.len() > IMAGE_MAX_SIZE as usize {
        return Err(format!("app image is {} bytes; must be 1..={}", app.len(), IMAGE_MAX_SIZE));
    }
    // The image's first instruction is `b _reset`, so entry == IMAGE_BASE.
    Ok(AppHeader { length: app.len() as u32, crc32: crc32(app), entry: IMAGE_BASE })
}

/// Builds the full flash image: bootloader at 0, header page, app image.
fn pack(boot: &[u8], app: &[u8]) -> Result<Vec<u8>, String> {
    let app = &word_padded(app);
    if boot.is_empty() || boot.len() > BOOT_SIZE as usize {
        return Err(format!("bootloader is {} bytes; must fit in {}", boot.len(), BOOT_SIZE));
    }
    let header = header_for(app)?;
    let mut img = vec![0xFFu8; 256 * 1024];
    img[..boot.len()].copy_from_slice(boot);
    let h = (APP_BASE - FLASH_BASE) as usize;
    img[h..h + HEADER_SIZE].copy_from_slice(&header.to_bytes());
    let i = (IMAGE_BASE - FLASH_BASE) as usize;
    img[i..i + app.len()].copy_from_slice(app);
    Ok(img)
}

struct Boot {
    h: DeviceHandle<GlobalContext>,
}

impl Boot {
    fn open() -> Result<Self, String> {
        let dev = rusb::devices()
            .map_err(|e| format!("listing USB devices: {e}"))?
            .iter()
            .find(|d| d.device_descriptor().map(|dd| dd.vendor_id() == USB_VID && dd.product_id() == USB_PID).unwrap_or(false))
            .ok_or("bootloader not found (hold Orange while powering on the brick)")?;
        let h = dev.open().map_err(|e| {
            format!("found the bootloader but cannot open it: {e} (permissions? install tools/nxtboot/99-nxtboot.rules, or run with sudo)")
        })?;
        let _ = h.set_auto_detach_kernel_driver(true);
        h.claim_interface(0).map_err(|e| format!("claim interface: {e}"))?;
        Ok(Self { h })
    }

    fn request(&mut self, req: Request, data: Option<&[u8]>) -> Result<Vec<u8>, String> {
        let (buf, n) = req.encode();
        self.h.write_bulk(EP_OUT, &buf[..n], TIMEOUT).map_err(|e| format!("send: {e}"))?;
        if let Some(d) = data {
            self.h.write_bulk(EP_OUT, d, TIMEOUT).map_err(|e| format!("send data: {e}"))?;
        }
        let mut reply = [0u8; PACKET_SIZE];
        let got = self.h.read_bulk(EP_IN, &mut reply, TIMEOUT).map_err(|e| format!("recv: {e}"))?;
        if got == 0 || reply[0] != Status::Ok as u8 {
            return Err(format!("device returned status {}", reply.first().copied().unwrap_or(255)));
        }
        Ok(reply[..got].to_vec())
    }
}

/// Asks the bootloader what it holds (the bootloader validates the CRC
/// against flash itself, so this reflects what is really stored).
fn installed_info() -> Result<Info, String> {
    let r = Boot::open()?.request(Request::Info, None)?;
    parse_info(&r).ok_or_else(|| "short INFO reply (old bootloader? reinstall it)".to_string())
}

fn flash(app: &[u8], boot: bool) -> Result<(), String> {
    let app = &word_padded(app);
    let header = header_for(app)?;
    let mut dev = Boot::open()?;
    dev.request(Request::Begin, None)?;
    for (i, chunk) in app.chunks(PAGE_SIZE).enumerate() {
        let mut page = [0xFFu8; PAGE_SIZE];
        page[..chunk.len()].copy_from_slice(chunk);
        let n = IMAGE_FIRST_PAGE + i as u16;
        dev.request(Request::Write { page: n }, Some(&page)).map_err(|e| format!("page {n}: {e}"))?;
        eprint!("\rwrote page {}/{}", i + 1, app.len().div_ceil(PAGE_SIZE));
    }
    eprintln!();
    dev.request(Request::Finish { header }, None)?;
    // FINISH already read the image back and checked its CRC on the device;
    // this second look uses the bootloader's INFO. An older bootloader
    // sends a short reply with no image details: the upload still stands.
    match parse_info(&dev.request(Request::Info, None)?) {
        Some(info) if info.app == Some((header.length, header.crc32)) => {
            println!("uploaded and confirmed in flash: {} bytes, crc32 {:08x}", header.length, header.crc32)
        }
        Some(info) => return Err(format!("upload finished but the bootloader reports {:?}", info.app)),
        None => println!(
            "uploaded {} bytes, crc32 {:08x} (device verified the CRC; this bootloader is too old to report it back -- reinstall it with just boot-install)",
            header.length, header.crc32
        ),
    }
    // The bootloader stays in upload mode until told otherwise, so start the
    // new app now rather than leaving the brick waiting for a power cycle.
    if boot {
        dev.request(Request::Boot, None)?;
        println!("booting the new app");
    }
    Ok(())
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let read = |p: &String| std::fs::read(p).map_err(|e| format!("{p}: {e}"));
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["info"] => {
            let info = installed_info()?;
            println!(
                "protocol v{}, page {} B, image from page {}, {} pages",
                info.version, info.page_size, info.image_first_page, info.flash_pages
            );
            match info.app {
                Some((len, crc)) => println!("installed app: {len} bytes, crc32 {crc:08x}"),
                None => println!("installed app: none (missing or failed validation)"),
            }
            Ok(())
        }
        ["verify", _] => {
            let want = header_for(&word_padded(&read(&args[1])?))?;
            match installed_info()?.app {
                Some((len, crc)) if (len, crc) == (want.length, want.crc32) => {
                    println!("MATCH: flash holds this image ({len} bytes, crc32 {crc:08x})");
                    Ok(())
                }
                Some((len, crc)) => Err(format!(
                    "DIFFERENT: flash holds {len} bytes crc32 {crc:08x}; {} has {} bytes crc32 {:08x}",
                    args[1], want.length, want.crc32
                )),
                None => Err("DIFFERENT: no valid app installed".into()),
            }
        }
        ["flash", _] => flash(&read(&args[1])?, true),
        ["flash", "--no-boot", _] => flash(&read(&args[2])?, false),
        ["boot"] => Boot::open()?.request(Request::Boot, None).map(|_| ()),
        ["pack", _, _, out] => {
            let img = pack(&read(&args[1])?, &read(&args[2])?)?;
            std::fs::write(out, img).map_err(|e| format!("{out}: {e}"))
        }
        _ => Err("usage: nxtboot info | verify APP.bin | flash APP.bin | boot | pack BOOT.bin APP.bin OUT.bin".into()),
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("nxtboot: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_layout_validates_on_device_side() {
        let boot = vec![0xB0; 100];
        let app: Vec<u8> = (0..1000u32).map(|i| i as u8).collect::<Vec<_>>()[..1000].to_vec();
        let img = pack(&boot, &app).unwrap();
        assert_eq!(img.len(), 256 * 1024);
        assert_eq!(&img[..100], &boot[..]);
        assert_eq!(img[100], 0xFF);
        // What the bootloader does: parse the header page, CRC the image.
        let h = (APP_BASE - FLASH_BASE) as usize;
        let header = AppHeader::parse(img[h..h + HEADER_SIZE].try_into().unwrap()).unwrap();
        let i = (IMAGE_BASE - FLASH_BASE) as usize;
        assert_eq!(header.verify(&img[i..i + header.length as usize]), Ok(()));
        assert!(img[h + HEADER_SIZE..i].iter().all(|&b| b == 0xFF));
    }

    #[test]
    fn pack_rejects_oversize_and_misaligned() {
        assert!(pack(&vec![0; BOOT_SIZE as usize + 1], &[0; 8]).is_err());
        assert!(pack(&[0; 8], &vec![0; IMAGE_MAX_SIZE as usize + 4]).is_err());
        // Odd lengths are padded to a word, not rejected.
        let img = pack(&[0; 8], &[0x11; 7]).unwrap();
        let i = (IMAGE_BASE - FLASH_BASE) as usize;
        assert_eq!(&img[i..i + 8], &[0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0xFF]);
        assert!(pack(&[0; 8], &[]).is_err());
    }
}

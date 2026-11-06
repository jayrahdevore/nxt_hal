//! Pure-logic pieces shared by the NXT bootloader and its host uploader:
//! flash layout constants, the app header, CRC32 and the command framing.
//!
//! No `nxt-pac` dependency, so everything here is covered by ordinary
//! host-side `cargo test` (see `docs/adr/0003-usb-bootloader.md`).
#![no_std]

/// First flash address (`AT91C_IFLASH`).
pub const FLASH_BASE: u32 = 0x0010_0000;
/// EFC page size in bytes (256 B pages, 1024 pages on the 256 KiB part).
pub const PAGE_SIZE: usize = 256;
/// Bootloader region: one 16 KiB lock region, pages 0..64.
pub const BOOT_SIZE: u32 = 16 * 1024;
/// App region starts right after the bootloader.
pub const APP_BASE: u32 = FLASH_BASE + BOOT_SIZE;
pub const FLASH_END: u32 = FLASH_BASE + 256 * 1024;
pub const APP_MAX_SIZE: u32 = FLASH_END - APP_BASE;
/// First flash page owned by the app (page numbers are relative to `FLASH_BASE`).
pub const APP_FIRST_PAGE: u16 = (BOOT_SIZE as usize / PAGE_SIZE) as u16;
pub const FLASH_PAGES: u16 = (256 * 1024 / PAGE_SIZE) as u16;

/// Placeholder USB IDs: pid.codes' open-source VID with its shared *test*
/// PID. Fine for bring-up on a bench; replace with a properly allocated PID
/// before this firmware is distributed.
pub const USB_VID: u16 = 0x1209;
pub const USB_PID: u16 = 0x0001;

/// An app asks the bootloader to enter upload mode on the next boot by
/// storing [`BOOT_REQUEST_MAGIC`] here (last word of SRAM, above the
/// stacks) and jumping to [`FLASH_BASE`]. The bootloader clears it.
pub const BOOT_REQUEST_ADDR: u32 = 0x0020_FFFC;
pub const BOOT_REQUEST_MAGIC: u32 = 0xB007_10AD;

/// Bulk packet size used on the data endpoints.
pub const PACKET_SIZE: usize = 64;

/// CRC-32/IEEE (reflected, poly 0xEDB88320, init/xorout 0xFFFFFFFF).
/// Bitwise rather than table-driven: the bootloader has 16 KiB total.
pub fn crc32(data: &[u8]) -> u32 {
    !crc32_update(!0, data)
}

/// Incremental form: start from `!0`, feed chunks, invert at the end.
/// Lets the bootloader checksum a large image in slices between AVR
/// services.
pub fn crc32_update(mut crc: u32, data: &[u8]) -> u32 {
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    crc
}

/// Bytes "NXTA" at the start of the header.
pub const HEADER_MAGIC: u32 = u32::from_le_bytes(*b"NXTA");

/// 16-byte header at `APP_BASE`, alone in its own page. The image proper
/// (vectors first) starts at the next page, `IMAGE_BASE`; `length` and `crc32` cover only that
/// image, not the header. The header page is written *last*, so an
/// interrupted upload leaves 0xFF there and never validates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AppHeader {
    pub length: u32,
    pub crc32: u32,
    /// Address of the app's `_reset`.
    pub entry: u32,
}

pub const HEADER_SIZE: usize = 16;
pub const IMAGE_BASE: u32 = APP_BASE + PAGE_SIZE as u32;
/// First page a WRITE may target (page `APP_FIRST_PAGE` is the header, written by FINISH).
pub const IMAGE_FIRST_PAGE: u16 = APP_FIRST_PAGE + 1;
pub const IMAGE_MAX_SIZE: u32 = APP_MAX_SIZE - PAGE_SIZE as u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeaderError {
    BadMagic,
    BadLength,
    BadEntry,
    BadCrc,
}

impl AppHeader {
    pub fn to_bytes(&self) -> [u8; HEADER_SIZE] {
        let mut out = [0u8; HEADER_SIZE];
        out[0..4].copy_from_slice(&HEADER_MAGIC.to_le_bytes());
        out[4..8].copy_from_slice(&self.length.to_le_bytes());
        out[8..12].copy_from_slice(&self.crc32.to_le_bytes());
        out[12..16].copy_from_slice(&self.entry.to_le_bytes());
        out
    }

    /// Checks magic, sizes and entry address only (cheap; no CRC).
    pub fn parse(bytes: &[u8; HEADER_SIZE]) -> Result<Self, HeaderError> {
        let w = |i: usize| u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
        if w(0) != HEADER_MAGIC {
            return Err(HeaderError::BadMagic);
        }
        let h = AppHeader {
            length: w(4),
            crc32: w(8),
            entry: w(12),
        };
        if h.length == 0 || h.length > IMAGE_MAX_SIZE {
            return Err(HeaderError::BadLength);
        }
        if h.entry < IMAGE_BASE || h.entry >= IMAGE_BASE + h.length || h.entry & 3 != 0 {
            return Err(HeaderError::BadEntry);
        }
        Ok(h)
    }

    /// Full validation against the image bytes as read back from flash.
    pub fn verify(&self, image: &[u8]) -> Result<(), HeaderError> {
        if image.len() != self.length as usize || crc32(image) != self.crc32 {
            return Err(HeaderError::BadCrc);
        }
        Ok(())
    }
}

/// Host -> device command packet: `[cmd, args...]`, at most [`PACKET_SIZE`]
/// bytes. `WRITE` is followed by exactly [`PAGE_SIZE`] raw data bytes (sent
/// as consecutive bulk packets). Every command is answered by one reply
/// packet whose first byte is a [`Status`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Request {
    /// Reply: status plus protocol version, flash layout and boot state
    /// (see [`encode_info`]).
    Info,
    /// Invalidate the app header page; must precede any WRITE.
    Begin,
    /// Program image page `page` (absolute page number, >= [`IMAGE_FIRST_PAGE`]).
    Write { page: u16 },
    /// Verify the image against `header` by reading it back from flash, then
    /// write the header page.
    Finish { header: AppHeader },
    /// Validate the app and jump to it.
    Boot,
}

pub const CMD_INFO: u8 = 1;
pub const CMD_BEGIN: u8 = 2;
pub const CMD_WRITE: u8 = 3;
pub const CMD_FINISH: u8 = 4;
pub const CMD_BOOT: u8 = 5;

pub const PROTOCOL_VERSION: u8 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Ok = 0,
    BadCommand = 1,
    BadPage = 2,
    /// WRITE or FINISH without a preceding BEGIN.
    NotBegun = 3,
    FlashError = 4,
    BadHeader = 5,
    /// Image read-back CRC didn't match the header.
    BadCrc = 6,
    /// BOOT with no valid app installed.
    NoApp = 7,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseError {
    TooShort,
    Unknown(u8),
    /// FINISH carried a header that fails [`AppHeader::parse`].
    BadHeader(HeaderError),
}

/// Size of the largest encoded request.
pub const MAX_REQUEST: usize = 1 + HEADER_SIZE;

impl Request {
    pub fn parse(pkt: &[u8]) -> Result<Self, ParseError> {
        let Some(&cmd) = pkt.first() else {
            return Err(ParseError::TooShort);
        };
        match cmd {
            CMD_INFO => Ok(Request::Info),
            CMD_BEGIN => Ok(Request::Begin),
            CMD_WRITE => {
                if pkt.len() < 3 {
                    return Err(ParseError::TooShort);
                }
                Ok(Request::Write {
                    page: u16::from_le_bytes([pkt[1], pkt[2]]),
                })
            }
            CMD_FINISH => {
                let body: &[u8; HEADER_SIZE] = pkt
                    .get(1..1 + HEADER_SIZE)
                    .and_then(|b| b.try_into().ok())
                    .ok_or(ParseError::TooShort)?;
                let header = AppHeader::parse(body).map_err(ParseError::BadHeader)?;
                Ok(Request::Finish { header })
            }
            CMD_BOOT => Ok(Request::Boot),
            other => Err(ParseError::Unknown(other)),
        }
    }

    pub fn encode(&self) -> ([u8; MAX_REQUEST], usize) {
        let mut out = [0u8; MAX_REQUEST];
        match *self {
            Request::Info => {
                out[0] = CMD_INFO;
                (out, 1)
            }
            Request::Begin => {
                out[0] = CMD_BEGIN;
                (out, 1)
            }
            Request::Write { page } => {
                out[0] = CMD_WRITE;
                out[1..3].copy_from_slice(&page.to_le_bytes());
                (out, 3)
            }
            Request::Finish { header } => {
                out[0] = CMD_FINISH;
                out[1..].copy_from_slice(&header.to_bytes());
                (out, MAX_REQUEST)
            }
            Request::Boot => {
                out[0] = CMD_BOOT;
                (out, 1)
            }
        }
    }
}

/// Size of the INFO reply.
pub const INFO_REPLY_SIZE: usize = 20;

/// INFO reply: `[status, version, app_valid, 0, page_size u16,
/// image_first_page u16, flash_pages u16, 0, 0, app_length u32, app_crc32 u32]`.
/// The last two are the installed (and validated) image's header values, 0
/// when no valid app is installed -- the host compares them against a local
/// file to prove what is actually in flash.
pub fn encode_info(app: Option<AppHeader>) -> [u8; INFO_REPLY_SIZE] {
    let mut out = [0u8; INFO_REPLY_SIZE];
    out[0] = Status::Ok as u8;
    out[1] = PROTOCOL_VERSION;
    out[2] = app.is_some() as u8;
    out[4..6].copy_from_slice(&(PAGE_SIZE as u16).to_le_bytes());
    out[6..8].copy_from_slice(&IMAGE_FIRST_PAGE.to_le_bytes());
    out[8..10].copy_from_slice(&FLASH_PAGES.to_le_bytes());
    if let Some(h) = app {
        out[12..16].copy_from_slice(&h.length.to_le_bytes());
        out[16..20].copy_from_slice(&h.crc32.to_le_bytes());
    }
    out
}

/// Decoded INFO reply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Info {
    pub version: u8,
    pub page_size: u16,
    pub image_first_page: u16,
    pub flash_pages: u16,
    /// `(length, crc32)` of the installed image, if valid.
    pub app: Option<(u32, u32)>,
}

/// Parses an INFO reply; `None` if too short (e.g. an older bootloader).
pub fn parse_info(r: &[u8]) -> Option<Info> {
    if r.len() < INFO_REPLY_SIZE {
        return None;
    }
    let u16_at = |i: usize| u16::from_le_bytes([r[i], r[i + 1]]);
    let u32_at = |i: usize| u32::from_le_bytes([r[i], r[i + 1], r[i + 2], r[i + 3]]);
    Some(Info {
        version: r[1],
        page_size: u16_at(4),
        image_first_page: u16_at(6),
        flash_pages: u16_at(8),
        app: (r[2] != 0).then(|| (u32_at(12), u32_at(16))),
    })
}

/// True if a WRITE may target `page` (never the bootloader, never the header page).
pub fn page_in_image_region(page: u16) -> bool {
    (IMAGE_FIRST_PAGE..FLASH_PAGES).contains(&page)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_known_vectors() {
        assert_eq!(crc32(b""), 0);
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        let part = crc32_update(crc32_update(!0, b"1234"), b"56789");
        assert_eq!(!part, 0xCBF4_3926);
    }

    #[test]
    fn layout_is_consistent() {
        assert_eq!(APP_FIRST_PAGE, 64);
        assert_eq!(APP_BASE, 0x0010_4000);
        assert_eq!(APP_MAX_SIZE, 240 * 1024);
        assert_eq!(IMAGE_BASE, 0x0010_4100);
        assert_eq!(IMAGE_FIRST_PAGE, 65);
        assert!(!page_in_image_region(63));
        assert!(!page_in_image_region(64)); // header page: FINISH only
        assert!(page_in_image_region(65));
        assert!(page_in_image_region(1023));
        assert!(!page_in_image_region(1024));
    }

    fn sample() -> AppHeader {
        AppHeader {
            length: 100,
            crc32: 0x1234_5678,
            entry: IMAGE_BASE,
        }
    }

    #[test]
    fn header_round_trip() {
        let h = sample();
        assert_eq!(AppHeader::parse(&h.to_bytes()), Ok(h));
    }

    #[test]
    fn erased_header_is_rejected() {
        assert_eq!(
            AppHeader::parse(&[0xFF; HEADER_SIZE]),
            Err(HeaderError::BadMagic)
        );
    }

    #[test]
    fn header_rejects_bad_fields() {
        let mut h = sample();
        h.length = 0;
        assert_eq!(AppHeader::parse(&h.to_bytes()), Err(HeaderError::BadLength));
        h.length = IMAGE_MAX_SIZE + 1;
        assert_eq!(AppHeader::parse(&h.to_bytes()), Err(HeaderError::BadLength));
        let mut h = sample();
        h.entry = IMAGE_BASE - 4;
        assert_eq!(AppHeader::parse(&h.to_bytes()), Err(HeaderError::BadEntry));
        h.entry = IMAGE_BASE + 100;
        assert_eq!(AppHeader::parse(&h.to_bytes()), Err(HeaderError::BadEntry));
        h.entry = IMAGE_BASE + 2;
        assert_eq!(AppHeader::parse(&h.to_bytes()), Err(HeaderError::BadEntry));
    }

    #[test]
    fn verify_checks_crc_and_length() {
        let image = [0xA5u8; 100];
        let h = AppHeader {
            length: 100,
            crc32: crc32(&image),
            entry: IMAGE_BASE,
        };
        assert_eq!(h.verify(&image), Ok(()));
        let mut bad = image;
        bad[7] ^= 1;
        assert_eq!(h.verify(&bad), Err(HeaderError::BadCrc));
        assert_eq!(h.verify(&image[..99]), Err(HeaderError::BadCrc));
    }

    #[test]
    fn request_round_trip() {
        for r in [
            Request::Info,
            Request::Begin,
            Request::Write { page: 0x03FF },
            Request::Finish { header: sample() },
            Request::Boot,
        ] {
            let (buf, n) = r.encode();
            assert_eq!(Request::parse(&buf[..n]), Ok(r));
        }
        assert_eq!(Request::parse(&[]), Err(ParseError::TooShort));
        assert_eq!(Request::parse(&[CMD_WRITE, 1]), Err(ParseError::TooShort));
        assert_eq!(Request::parse(&[9]), Err(ParseError::Unknown(9)));
        // FINISH with a truncated or invalid header.
        assert_eq!(
            Request::parse(&[CMD_FINISH, 1, 2]),
            Err(ParseError::TooShort)
        );
        let mut bad = [0u8; MAX_REQUEST];
        bad[0] = CMD_FINISH;
        assert_eq!(
            Request::parse(&bad),
            Err(ParseError::BadHeader(HeaderError::BadMagic))
        );
    }

    #[test]
    fn info_reply_layout() {
        let h = AppHeader {
            length: 100,
            crc32: 0xDEAD_BEEF,
            entry: IMAGE_BASE,
        };
        let i = parse_info(&encode_info(Some(h))).unwrap();
        assert_eq!(i.version, PROTOCOL_VERSION);
        assert_eq!(
            (i.page_size, i.image_first_page, i.flash_pages),
            (256, 65, 1024)
        );
        assert_eq!(i.app, Some((100, 0xDEAD_BEEF)));
        assert_eq!(parse_info(&encode_info(None)).unwrap().app, None);
        assert_eq!(parse_info(&[0; 12]), None); // older bootloader
    }
}

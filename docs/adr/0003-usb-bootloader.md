# 0003: USB bootloader

Status: implemented, **unverified on hardware**.

## Why
Updating firmware required SAM-BA mode (hold the hidden reset button 5+ s)
and a whole-chip `fwflash`. `firmware/nxt-boot` is a resident bootloader
that accepts a new app over USB when Orange is held at power-on.
SAM-BA remains the recovery path.

## Flash layout
| Range | Contents |
|---|---|
| `0x00100000..0x00104000` | bootloader (16 KiB = EFC lock region 0), owns the vector table at address 0 |
| `0x00104000..0x00104100` | app header page (`NXTA`, length, CRC32, entry), written last |
| `0x00104100..0x00140000` | app image, linked with `ld/link-app.x` |

Constants live in `crates/nxt-boot-proto` (host-tested). ARM7 has no VTOR,
so the bootloader keeps the only vector table; the app's `.vectors` is
just a `b _reset` at its first word, which is the header's `entry`. The
exception vectors other than reset are `b .` stubs in both images today;
**forwarding IRQ/FIQ to the app must be added together with the AIC**.

## Boot decision
1. `wdt.disable()`, `init_clocks()`, bring up the AVR link, wait (<=1 s) for
   the first valid reply -- before that the button reads 0.
2. Validate the app (header fields + CRC32 of the image, chunked so the AVR
   stays serviced). Invalid/missing -> upload mode.
3. If the link is up and Orange stays down for 1.5 s -> upload mode.
   Releasing early boots the app. (Powering on *is* an Orange press; a
   normal quick press is released well before 1.5 s. Tune `HOLD_MS` after
   measuring on hardware.) Apps that are already running instead enter
   upload mode when Left+Right is held (`Button::LeftRight` in
   `nxt_hal::avr`); only the bootloader itself uses Orange.
   (Before step 1's link wait, after the first blip, the bootloader draws
   a wrench splash on the LCD via `nxt_hal::lcd`; see ADR 0004. It is shown
   on every power-up, and an app that doesn't use the LCD leaves it up. The
   bitmap is `firmware/nxt-boot/src/wrench.rs`, generated from the
   hand-drawn `assets/wrench.svg` by `tools/img2lcd/convert.sh`. Cost: about
   1.3 KiB of the 16 KiB budget.)
4. Otherwise `Pmc::slow_clock()` (so the app's `init_clocks` never
   rewrites the PLL while MCK runs from it) and `jump_to`.

## Protocol (USB 1209:0001 placeholder, interface 0, EP1 OUT / EP2 IN, 64 B)
`INFO`, `BEGIN` (erases the header page), `WRITE page` + 256 data bytes,
`FINISH header` (reads the image back from flash, checks CRC, writes the
header page), `BOOT`. One reply packet per command, first byte = `Status`.
No separate erase command: the EFC write-page command erases first.

## Host
`tools/nxtboot` (own workspace; build from outside the repo because the
root `.cargo/config.toml` forces `build-std=core`): `info`, `flash`, `boot`,
`pack` (full 256 KiB image for the one-time `just boot-install`).

## Unverified / to check on hardware
- UDP enumeration and the PA16 pull-up polarity (active low, from NxOS).
- Whether flash programming (ms-long bus stalls) drops the AVR link.
- EFC lock command: `lock_region` passes `region*64` as the page number; the
  header comment says region is "bits 4 to 7 of PAGEN", which doesn't match
  64-page regions. Check `is_region_locked` after `SET_LOCK`. The
  `lock-boot` feature is off by default for this reason.
- `HOLD_MS` and the Orange-at-power-on interaction.
- PID 0x0001 is pid.codes' shared *test* PID: replace before distribution.

## Licensing note
Register layouts and the enumeration flow were read from the vendored NxOS
sources (a local, git-ignored clone of `danderson/nxos` at `vendor/nxos`, GPLv2). The code here is written independently, not
copied; keep it that way, or revisit the crates' MIT/Apache license.

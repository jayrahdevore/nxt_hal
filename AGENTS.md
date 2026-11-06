# nxt_hal

A `no_std`, bare-metal Rust HAL for the LEGO Mindstorms NXT. This crate
graph *is* the firmware — it replaces LEGO's stock firmware on the brick's
main MCU, flashed over the brick's built-in SAM-BA USB bootloader. It is
not a PC-side driver/SDK.

## Hardware

- Main MCU: Atmel/Microchip **AT91SAM7S256** — ARM7TDMI core (ARMv4T
  architecture), 256KB flash, 64KB SRAM, 48MHz.
- There is a secondary **AVR (ATmega48) co-processor** on the mainboard.
  Motor power/direction/brake is sent to it over I2C (TWI); the main ARM
  never drives a motor H-bridge directly. See
  `docs/adr/0001-motor-avr-link.md` for the full protocol, confirmed
  directly from the `nxos` open-source project's source.
- Tachometer/encoder reading, by contrast, *is* ARM-side: PIOA GPIO pins
  plus the AIC's edge-interrupt feature, software-decoded (not a hardware
  timer-counter capture channel). Also documented in that ADR.
- There is no SVD for this chip. Register addresses in `nxt-pac` come from
  Atmel's own generated header (`AT91SAM7S256.h`, as shipped in the
  `at91lib` BSP) — check that source (or re-derive from the datasheet)
  before trusting an address/offset that isn't already in this crate.

## Workspace layout

- `crates/nxt-pac` — hand-written register definitions ("PAC"). Maps only
  what the datasheet says the bits do; no policy.
- `crates/nxt-rt` — the ARM7TDMI equivalent of `cortex-m-rt`, which
  doesn't exist upstream for this architecture outside the GBA-specific
  Rust crates (`gba`/`agb`). Vector table + reset handler are hand-written
  in assembly (`global_asm!`) since ARM7TDMI has no NVIC and no automatic
  exception-entry register stacking — none of that is expressible in safe
  Rust.
- `crates/nxt-hal` — the safe, typestate-constrained public API
  (`embedded-hal` traits where they fit) that firmware actually depends
  on.
- `firmware/*` — thin `#![no_std] #![no_main]` binaries, one per milestone
  deliverable.
- `ld/` — linker scripts: `link.x` (default, flash at 0), `link-boot.x`
  (bootloader), `link-app.x` (app behind the bootloader), `link-ram.x`
  (RAM-loaded probes, see `tools/samba`).
- `tools/` — host-side tools, outside the cargo workspace (it forces
  `build-std` for armv4t): `nxtboot` (USB bootloader client, own
  `Cargo.lock`), `samba` (SAM-BA debug clients in C), `img2lcd` (SVG ->
  LCD bitmap converter for splash/Ferris assets).
- `vendor/` — git-ignored, local-only. `vendor/nxos` is a clone of
  `github.com/danderson/nxos` (GPLv2) kept as reading material; nothing is
  copied from it. Re-clone it if you need it.
- `docs/adr/` — decisions that took real research to pin down (e.g. the
  motor/AVR-link protocol); read these before re-deriving something
  that's already been nailed down.

## Target: `armv4t-none-eabi`

Tier-3 rustc target, originally added for Game Boy Advance homebrew (GBA
is also ARM7TDMI/ARMv4T) but general-purpose. No prebuilt `std`/`core`
artifacts ship for it — building requires nightly Rust with the
`rust-src` component and `-Z build-std=core` (already wired up in
`rust-toolchain.toml` and `.cargo/config.toml`). There's no `rustup
target add` step; `rust-src` is what lets `build-std` compile `core` from
source for it.

The linker of record is GNU `arm-none-eabi-ld`, not `rust-lld` — this
target ships with no default linker script, and the GBA ecosystem's
experience with the same CPU family favors GNU `ld` for this style of
hand-written linker script (`ld/link.x`).

## Build / flash

Enter the Nix shell first (`nix-shell`), which provides the Rust
toolchain, `arm-none-eabi-*` binutils, and `fwflash` (built from source —
not packaged in nixpkgs; see `nix/libnxt.nix`).

```
just build          # cargo build -Z build-std=core --release -p nxt-blinky
just bin            # objcopy ELF -> raw .bin in target/images/
just flash          # fwflash over USB (brick must be in SAM-BA mode first --
                     # hold the hidden reset button under the USB port 5+ seconds)
just size           # arm-none-eabi-size: check the 256K flash / 64K RAM budget
just disasm         # objdump -d: manually verify the vector table / section layout
just test           # host-side tests (test-host + test-tools)
just boot-build     # bootloader; also app-build/app-bin/boot-install/app-upload, ram-build
just clean-images   # remove target/images/
```

All recipes take an optional firmware name (default `nxt-blinky`); generated
images land in `target/images/`, never in the repo root.

## Verification limits — read before claiming something "works"

**No AT91SAM7S/NXT emulator exists.** QEMU's only ARMv4T-adjacent machine
model is for the GBA, which has a completely different memory map and
peripheral set, and isn't worth adapting here. That means:

- A clean `cargo build` and a correct-looking `objdump -d` (vector table
  bytes at `0x0`-`0x1C`, section placement matching `ld/link.x`) are the
  *only* checks available without physical hardware.
- Pure logic with no `nxt-pac` dependency (duty-cycle math, type
  conversions, tachometer arithmetic) should be tested with ordinary
  `cargo test` on the host — that's the one place host-side testing is
  genuinely useful here.
- Anything involving real peripheral behavior (does the pin actually
  toggle, does the motor actually spin, does the AVR link actually come
  up) can only be confirmed by flashing real hardware. Don't report such
  a change as verified without having done that, or say explicitly that
  it's unverified and needs a hardware test.

## Known gotchas

- The AVR co-processor must be serviced continuously (handshake, then an
  exchange every ~1 ms) or the brick clicks/pops and may power down. See
  `docs/adr/0002-bringup-findings.md`, which also documents the RAM-probe
  debugging workflow (`tools/samba`): hardware state *can* be read back
  over USB, so "only a flash test works" no longer fully holds.
- Peripheral IDs are easy to misremember (SSC=8, TWI=9); take them from the
  header, not memory.

- The AT91SAM7S watchdog is **enabled by default out of reset** with a
  short timeout. `wdt.disable()` must run before anything that could
  stall past that timeout — it's the classic "board never boots" trap on
  this chip.
- A peripheral with its PMC clock bit unset reads back all-zero and
  silently ignores writes. `nxt-hal`'s typestate API (e.g.
  `Pmc::enable_pioa_clock` returning the `ClockEnabled` proof `Pio::new`
  requires) exists specifically to make this a compile-time error instead
  of a silent runtime trap — don't bypass it by reaching into `nxt-pac`
  directly unless you're deliberately working below that safety line.

## USB bootloader

`firmware/nxt-boot` (see `docs/adr/0003-usb-bootloader.md`) lets you update
the app over USB. From a cold boot, hold Orange while powering on; from a
running app (`nxt-lcd`, `nxt-motor`, `nxt-blinky`), hold Left+Right. One-time install over
SAM-BA with `just boot-install`; afterwards `just app-upload`. Apps meant
for it link with `ld/link-app.x` (`just app-build`), not `ld/link.x`. The
bootloader itself is all hardware-unverified until flashed.

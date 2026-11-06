firmware := "nxt-blinky"
target_dir := "target/armv4t-none-eabi/release"
images := "target/images"

# Build a firmware binary (default: nxt-blinky).
build firmware=firmware:
    cargo build -Z build-std=core --release -p {{firmware}}

# Convert the built ELF to a raw binary fwflash can push over USB.
# fwflash (libnxt) rejects anything that isn't exactly 256 KiB, so pad the
# image out to the end of flash (0x00100000 + 256K) with erased-flash 0xFF.
bin firmware=firmware: (build firmware)
    mkdir -p {{images}}
    arm-none-eabi-objcopy -O binary --pad-to 0x00140000 --gap-fill 0xff {{target_dir}}/{{firmware}} {{images}}/{{firmware}}.bin

# Flash over the NXT's built-in SAM-BA USB bootloader. Put the brick into
# SAM-BA mode first: hold the hidden reset button under the USB port for
# 5+ seconds until it clicks and re-enumerates.
flash firmware=firmware: (bin firmware)
    fwflash {{images}}/{{firmware}}.bin

# Report how much of the 256K flash / 64K RAM budget the image uses.
size firmware=firmware: (build firmware)
    arm-none-eabi-size {{target_dir}}/{{firmware}}

# Disassemble, to manually check the vector table and section placement
# against ld/link.x's intent -- the only offline check available, since no
# AT91SAM7S/NXT emulator exists.
disasm firmware=firmware: (build firmware)
    arm-none-eabi-objdump -d {{target_dir}}/{{firmware}} | less

# Host-side unit tests for pure logic (no nxt-pac dependency). The
# workspace config forces build-std for armv4t, so compile with rustc
# directly instead of `cargo test`.
test-host:
    mkdir -p target/host
    rustc --edition 2021 --test crates/nxt-boot-proto/src/lib.rs -o target/host/nxt-boot-proto-tests
    target/host/nxt-boot-proto-tests

# Run the host-tool tests.
test-tools:
    cd / && cargo test --manifest-path {{justfile_directory()}}/tools/nxtboot/Cargo.toml --target-dir {{justfile_directory()}}/target/host-tools

# All host-side tests.
test: test-host test-tools

# Remove generated .bin images (the build cache in target/ is untouched).
clean-images:
    rm -rf {{images}}

# ---- USB bootloader (see docs/adr/0003-usb-bootloader.md) ----
# The bootloader and app link with different scripts than the default
# ld/link.x, so these override RUSTFLAGS (which replaces the config's flags)
# and use their own target dirs to avoid thrashing the build-std cache.
boot_dir := "target/boot/armv4t-none-eabi/release"
app_dir := "target/app/armv4t-none-eabi/release"
nxtboot := "cd / && cargo run -q --manifest-path " + justfile_directory() + "/tools/nxtboot/Cargo.toml --target-dir " + justfile_directory() + "/target/host-tools --"
images_abs := justfile_directory() + "/" + images

# Build `pkg` with linker script `script` into target/`dir`.
[private]
_link-build script dir pkg:
    RUSTFLAGS="-C link-arg=-Tld/{{script}} -C panic=abort" cargo build -Z build-std=core --release -p {{pkg}} --target-dir target/{{dir}}

# Build the bootloader (16 KiB budget).
boot-build: (_link-build "link-boot.x" "boot" "nxt-boot")
    arm-none-eabi-size {{boot_dir}}/nxt-boot

# Build `firmware` to run behind the bootloader (default: nxt-blinky).
app-build firmware=firmware: (_link-build "link-app.x" "app" firmware)

app-bin firmware=firmware: (app-build firmware)
    mkdir -p {{images}}
    arm-none-eabi-objcopy -O binary {{app_dir}}/{{firmware}} {{images}}/{{firmware}}.app.bin

# One-time install over SAM-BA: bootloader + current app as a full image.
# Hold the reset button 5+ s first, as for `just flash`.
boot-install firmware=firmware: boot-build (app-bin firmware)
    arm-none-eabi-objcopy -O binary {{boot_dir}}/nxt-boot {{images}}/nxt-boot.bin
    {{nxtboot}} pack {{images_abs}}/nxt-boot.bin {{images_abs}}/{{firmware}}.app.bin {{images_abs}}/nxt-boot-full.bin
    fwflash {{images}}/nxt-boot-full.bin

# Update the app over USB: hold Orange while powering on the brick (or
# Left+Right from a running app).
app-upload firmware=firmware: (app-bin firmware)
    {{nxtboot}} flash {{images_abs}}/{{firmware}}.app.bin

# Build `nxt-avrprobe` for RAM (ld/link-ram.x); `load` it at 0x202000 with
# tools/samba (see tools/samba/README.md).
ram-build: (_link-build "link-ram.x" "ram" "nxt-avrprobe")
    mkdir -p {{images}}
    arm-none-eabi-objcopy -O binary target/ram/armv4t-none-eabi/release/nxt-avrprobe {{images}}/nxt-avrprobe.ram.bin

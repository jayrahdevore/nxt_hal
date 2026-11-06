firmware := "nxt-blinky"
target_dir := "target/armv4t-none-eabi/release"

# Build a firmware binary (default: nxt-blinky).
build firmware=firmware:
    cargo build -Z build-std=core --release -p {{firmware}}

# Convert the built ELF to a raw binary fwflash can push over USB.
# fwflash (libnxt) rejects anything that isn't exactly 256 KiB, so pad the
# image out to the end of flash (0x00100000 + 256K) with erased-flash 0xFF.
bin firmware=firmware: (build firmware)
    arm-none-eabi-objcopy -O binary --pad-to 0x00140000 --gap-fill 0xff {{target_dir}}/{{firmware}} {{firmware}}.bin

# Flash over the NXT's built-in SAM-BA USB bootloader. Put the brick into
# SAM-BA mode first: hold the hidden reset button under the USB port for
# 5+ seconds until it clicks and re-enumerates.
flash firmware=firmware: (bin firmware)
    fwflash {{firmware}}.bin

# Report how much of the 256K flash / 64K RAM budget the image uses.
size firmware=firmware: (build firmware)
    arm-none-eabi-size {{target_dir}}/{{firmware}}

# Disassemble, to manually check the vector table and section placement
# against link.x's intent -- the only offline check available, since no
# AT91SAM7S/NXT emulator exists.
disasm firmware=firmware: (build firmware)
    arm-none-eabi-objdump -d {{target_dir}}/{{firmware}} | less

# SAM-BA debug tools

Tiny libusb clients for the NXT's SAM-BA bootloader (03eb:6124), used to
debug firmware without JTAG. Build each with
`gcc X.c -o X -include unistd.h $(pkg-config --cflags --libs libusb-1.0)`.

- `samba ADDR N [ADDR N ...]` -- read N words at hex ADDR.
- `poke rADDR wADDR=VAL sMS ...` -- read/write registers, sleep (hex).
- `load FILE ADDR` -- write a raw binary into RAM and run it (`G`).

Workflow: `just ram-build` (writes `target/images/nxt-avrprobe.ram.bin`),
`load` it at 202000, then `samba 20D000 N` to read
its mailbox. See `docs/adr/0002-bringup-findings.md` for the gotchas.

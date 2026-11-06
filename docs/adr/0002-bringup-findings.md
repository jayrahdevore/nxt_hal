# ADR 0002: Clock, AVR link and sound bring-up (hardware-verified)

Status: verified on a real brick (popping stopped, startup jingle plays
with correct pitch and timing).
References: NxOS (`github.com/danderson/nxos`: `init.S`, `avr.c`, `_twi.c`,
`sound.c`, `systick.c`).

## Findings

- **The "clicking/popping" is the AVR waiting for the ARM.** It repeats
  about once per second until the ARM sends the TWI handshake, and also
  sounds the same in SAM-BA mode. It is not a crash signature.
- **Peripheral IDs:** SSC = 8 (not 4, which is the ADC), TWI = 9, PIOA = 2.
  A wrong ID silently enables the wrong clock; the SSC then never sets
  TXRDY.
- **Clocks:** the SAM-BA monitor runs MCK from the 18.432 MHz main osc
  (MCKR=1). `Pmc::init_clocks` (NxOS recipe: FMR FWS=1/FMCN=72, MOR=0x601,
  PLLR=0x10481C0E, MCKR 0x4 then 0x7) gives MCK ~= 48.05 MHz.
- **AVR link:** handshake string, then alternate 9-byte write / 13-byte
  read, >= 1 ms apart. The AVR **drops the link if it is not serviced for
  a few ms**; once dropped it ignores reads until re-greeted. The driver
  re-handshakes after 3 failures (as NxOS) and is non-blocking
  (`AvrLink::service`, one transaction per PIT tick).
- **Sound:** SSC TD on PA17, peripheral A. Feeding the FIFO from the CPU
  while also servicing the AVR underruns and sounds like white noise;
  stream with the SSC's PDC (chained 16-word buffers, `Speaker::beep`).
- **Tone divider:** bit clock = MCK / (2 * CMR), one tone period = 512
  bits, so CMR = MCK / (1024 * freq). NxOS's constant assumes the
  undivided 96.11 MHz PLL, which came out an octave low here; one cycle
  lasts 1/freq s, so a note of `ms` needs `freq * ms / 1000` cycles.
- All waits on hardware flags are bounded; a mis-set register must never
  hang the firmware.

## Debugging method (no JTAG)

Run a short probe from RAM through SAM-BA `G`, have it write a mailbox and
**return** to the monitor, then read it back (`tools/samba`). Rules learnt:

- Keep probes well under ~1 s: while the probe runs the monitor cannot
  service USB and the host resets the device (`error -110` in dmesg).
- A held reset button cuts power, wiping SRAM: breadcrumbs written by
  flashed firmware are lost; RAM probes are the only readback.
- `fwflash` fails right after a probe (clock changed); re-enter SAM-BA
  before flashing.

- Idle hiss: with the SSC left enabled after a tone, PA17 (TD) parks at a
  constant high level (probe: 0 edges in 20k reads, SSC idle) and the
  speaker hissed quietly and constantly. Fix: after each tone set
  `CR_TXDIS` and drive PA17 low as a GPIO; `beep()` re-claims it. The user
  confirmed by ear that the hiss is gone.

- Buttons: the AVR reply carries one analog level at rx[8..10] (idle 0..5,
  orange ~2048). All four buttons register as pressed with the NxOS-derived
  thresholds; only orange's level was measured, so the other three variant
  names are unverified.

## Unverified (from memory, not cross-checked against the header)

SSC base `0xFFFD4000` and register offsets; `TCMR`/`TFMR` bit values; the
SSC PDC offsets. They work on hardware, but have not been checked against
`AT91SAM7S256.h`.

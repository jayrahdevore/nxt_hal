# ADR 0001: Motor control goes through the AVR co-processor over I2C, not direct ARM PWM

## Status

Accepted. This resolves the Milestone 1 research spike from the project plan.

## Context

The NXT mainboard has a secondary AVR (ATmega48) co-processor alongside the
main AT91SAM7S256 (ARM7TDMI). Before designing `nxt-hal`'s `Motor` API, we
needed to know whether motor speed/direction/brake is:

- **(A)** driven directly by the ARM via its own PWM controller (PWMC) and
  GPIO H-bridge lines, or
- **(B)** sent as commands to the AVR over some bus, with the AVR actually
  driving the H-bridges.

Secondary sources disagreed during initial research (an ecrobot/nxtOSEK doc
claimed I2C; an earlier, less careful pass over this same question
initially mis-reported the bus as UART/SPI). This was resolved by reading
the actual source of the open-source `nxos` project
(github.com/dacav/nxos), specifically `base/drivers/avr.c`, `_avr.h`,
`motors.c`, and `motors.h`.

## Decision

**Design B is correct: motor power/direction/brake is sent to the AVR over
I2C (the AT91's TWI peripheral); the ARM never drives a motor H-bridge
directly.** `nxt-hal`'s `MotorBackend` trait (per the project plan) should
be implemented against a TWI link to the AVR, not against PWMC.

**Tachometer/encoder reading is ARM-side, but not via TC0/1/2 as originally
assumed in the plan — it's PIOA GPIO + AIC edge-interrupts with a
software-decoded counter.** This is independent of the AVR-link question
and doesn't need to wait on it.

### ARM↔AVR link (from `avr.c`/`_avr.h`)

- Bus: TWI (I2C), slave address `1` (`AVR_ADDRESS`).
- Driven from a 1ms systick tick (`nx__avr_fast_update`), running a state
  machine (`AVR_LINK_DOWN → AVR_INIT → AVR_WAIT_2MS → AVR_WAIT_1MS →
  AVR_SEND ⇄ AVR_RECV`).
- Link-up handshake: writes a fixed string
  `"\xCC" "Let's samba nxt arm in arm, (c)LEGO System A/S"` over TWI, then
  cycles write/read forever.
- Outgoing packet (`raw_to_avr`, 9 bytes):
  | Byte | Meaning |
  |---|---|
  | 0 | Power-mode command (0 normal; 0x5A signals a power-off/reset request) |
  | 1 | PWM frequency (8 in normal run mode) / second half of power command |
  | 2-4 | Signed `i8` speed (-100..100) per motor, A/B/C |
  | 5 | Unused 4th-motor slot, hardcoded 0 ("required" by the AVR firmware per the nxos source comment) |
  | 6 | 3-bit brake bitmask, one bit per motor |
  | 7 | Sensor/input power modes -- unimplemented in nxos (always 0) |
  | 8 | Checksum: bitwise NOT of the sum of bytes 0-7 |
- Incoming packet (`raw_from_avr`, 13 bytes): 4x `u16` analog sensor
  readings, 1x `u16` buttons reading (thresholded into
  OK/CANCEL/RIGHT/LEFT/NONE), 1x `u16` mixed word (AVR firmware
  major/minor version, battery-type flag, raw battery voltage scaled by
  `×3545>>9 ≈ ×13.848mV`), 1 checksum byte (valid iff the unsigned sum of
  all 13 bytes is `0xFF`).
- `nx__avr_set_motor(motor, power_percent, brake)` just updates local state;
  the transfer happens on the next 1ms tick.

### Encoder reading (from `motors.c`)

```c
#define MOTOR_A_TACH AT91C_PIO_PA15   #define MOTOR_A_DIR AT91C_PIO_PA1
#define MOTOR_B_TACH AT91C_PIO_PA26   #define MOTOR_B_DIR AT91C_PIO_PA9
#define MOTOR_C_TACH AT91C_PIO_PA0    #define MOTOR_C_DIR AT91C_PIO_PA8
```

Setup: PIOA clock enabled, glitch filter on the tach/dir pins, pull-ups
disabled (externally driven), pins set as PIO inputs, then an AIC
interrupt is installed on `AT91C_ID_PIOA` with `PIOA_IER` unmasking only
the 3 TACH pins. The ISR reads `PIOA_ISR` (which pins just changed) and
`PIOA_PDSR` (current levels); for each motor whose TACH pin changed, it
compares the TACH bit to the DIR bit -- differ means increment, match
means decrement. This is a software quadrature decode on a single
PIO-bank interrupt source, not a hardware timer-counter capture channel.

## Consequences

- `nxt-pac` needs a **TWI** register module (not previously written) before
  `Motor::set_power` can be implemented, plus an **AIC** module and a way
  for `nxt-rt`'s currently-stubbed `_irq_handler` to dispatch to an
  installed ISR (Milestone 0/1 run with IRQ masked and don't need this
  yet).
- `MotorBackend`'s concrete implementation sends the 9-byte packet
  described above and parses the 13-byte reply; `Motor::set_power` is
  therefore `Result<(), MotorError>` (an I2C transaction can NAK or time
  out), matching "Design B" in the project plan.
- `Encoder` can be built independently of the above, directly against
  PIOA + AIC, once AIC support exists in `nxt-rt`/`nxt-pac`.
- The exact PWM-frequency byte (`8` in nxos) and the checksum algorithms
  above should be taken as given (they're copied verbatim from a working
  reference implementation) rather than re-derived.

## References

- `nxos/base/drivers/avr.c`, `_avr.h`, `motors.c`, `motors.h` --
  `github.com/dacav/nxos` (fork of the original `danderson/nxos`).

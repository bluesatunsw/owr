# Stepper drivebase firmware

See also `main.rs` (and maybe `Cargo.toml`).

## Firmware organisation

This crate uses the `embedded_common` crate, which should be shipped and
checked out in this crate's parent directory (see `Cargo.toml`). This provides
STM32G4-specific drivers that are reused across Bluesat firmware for different
boards (e.g. Cyphal/canadensis FDCAN driver), and some other tools (e.g.
OpenOCD config files), which should be clear reading the following section.

Otherwise, we have a pretty normal-style flat module hierarchy.

## Running and debugging the firmware

To run the firmware in a debugging/development context with an ST-LINK connected:

```
openocd -f ../embedded_common/tools/stm32g4x.cfg
cargo run --release
```

It's pretty much necessary to run in release mode due to space optimisations
being necessary to fit onto the dozens of KB of device flash.

To flash the firmware *and have it continue to run after you disconnect the
debugger* (for integration testing, or the competition (!)), do *not* run
OpenOCD. Instead, with the debugger connected, run:

```
cargo flash --release --chip stm32g474rbtx
```

(You may need to install `cargo flash`.)

Ideally, we would add extra configuration to the different release profiles
(i.e. `debug` and `release`) so that `debug` actually fits into the flash *and*
provides a nonzero amount of helpful debug information, but we haven't gotten
around to doing this yet. (Keen beans, take note!)

## A note on DSDL

The `dsdl/` directory has been removed since we don't currently plan to use any
non-publically regulated types. If we ever do need them then just create the
directory again and add the types as in the `canadensis_mvp` crate (available
at all good Bluesat repositories near you).

## Message IDs for Cyphal

For the stepper motor, subject-IDs for drive commands are in the form `30xy`:

- `30` means the message is intended for the drivebase
- `x` is the channel identifier:
    - 0 to 3 refer to the front-left (FL), front-right (FR), back-left (BL) and
      back-right (BR) motors respectively (in the context of the drivebase).
      These also correspond to the CH0 to CH3 designations as printed on the
      board's silkscreen.
- `y` is the "topic", which currently has underspecified semantics. Just use `0`.

A message with subject-ID 3020, for example, is a drive command targeting the
back-left stepper motor.

## Runtime diagnostics

The node's `GetInfo` response includes the first 64 bits of the firmware's Git
revision. `0` means the revision could not be determined during the build.
The same revision is included in semihosting panic output. The revision
identifies the commit, not uncommitted working-tree changes.

The node reports the first runtime firmware error as an advisory health status
with one of these vendor status codes:

- `0xF0`: invalid steering setpoint
- `0xF1`: stepper SPI failure
- `0xF2`: CAN receive failure
- `0xF3`: CAN transmit/publish failure
- `0xF4`: invalid execute-command request

These codes remain set until reboot so transient failures remain visible in
subsequent heartbeats. Stepper health is polled every 100 ms rather than on
every main-loop iteration.

## A historical note

This firmware formerly had provisions for being flashed to multiple different
chips across the STM32G4 and G0 families, with the target board being specified
as a build feature. This mechanism is demonstrated in `canadensis_mvp`. To
avoid stretching development effort too thinly, this has since been removed.
The current policy is that this firmware will always target and be maintained
solely for the latest revision of the board, which as of the time of writing
is:

Rev. 3p0

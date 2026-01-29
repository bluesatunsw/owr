# CPM firmware

It just runs led's cro :sob:

See also `main.rs` (and maybe `Cargo.toml`).

## Running and debugging the firmware

`openocd -f openocd/stm32g4x.cfg` and `cargo run --release if you have an
ST-LINK connected. It's pretty much necessary to run in release mode due to
space optimisations being necessary to fit onto the device.

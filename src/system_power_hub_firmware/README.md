# System power module firmware

I copied ts off the mpm firmware lowk

`openocd -f openocd/stm32g4x.cfg` and `cargo run --feature rev_a1` if you have an ST-LINK connected.
These should be run in seperate terminals with semihosting printing to the openocd terminal.

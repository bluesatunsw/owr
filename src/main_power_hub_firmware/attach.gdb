# change this path if you change the crate name!
file target/thumbv7em-none-eabihf/release/main_power_hub_firmware
# print demangled symbols
set print asm-demangle on
# detect unhandled exceptions, hard faults and panics
break DefaultHandler
break HardFault
# *try* to stop at the user entry point (it might be gone due to inlining)
break main
target extended-remote :1337

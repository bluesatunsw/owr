# change this path if you change the crate name!
file target/thumbv7em-none-eabihf/release/g474rbt6-minimal
target extended-remote :1337
# print demangled symbols
set print asm-demangle on
# detect unhandled exceptions, hard faults and panics
break DefaultHandler
break HardFault
# reflash program
load
# start and immediately halt the processor
stepi

//! This build script copies the appropriate `memory.x` file from the linker
//! directory into a directory where the linker can always find it at build time.
//!
//! The build script also sets the linker flags to tell it which link script to use.

use std::env;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

fn main() {
    // Put the appropriate `memory.x` in our output directory and ensure it's
    // on the linker search path.
    let out = &PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let mut f = File::create(out.join("memory.x")).unwrap();

    f.write_all(include_bytes!("linker/memory_stm32g474.x"))
        .unwrap();
    println!("cargo:rerun-if-changed=linker/memory_stm32g474.x");

    println!("cargo:rustc-link-search={}", out.display());

    // Specify linker arguments.

    // `--nmagic` is required if memory section addresses are not aligned to 0x10000,
    // for example the FLASH and RAM sections in your `memory.x`.
    // See https://github.com/rust-embedded/cortex-m-quickstart/pull/95
    //println!("cargo:rustc-link-arg=--nmagic");

    // Set the linker script to the one provided by cortex-m-rt.
    println!("cargo:rustc-link-arg=-Tlink.x");
}

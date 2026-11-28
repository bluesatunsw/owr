//! This build script copies the appropriate `memory.x` file from the linker
//! directory into a directory where the linker can always find it at build time.
//!
//! The build script also sets the linker flags to tell it which link script to use.

use std::env;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let revision = Command::new("git")
        .args(["rev-parse", "--verify", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|revision| revision.trim().to_owned())
        .filter(|revision| revision.len() >= 16)
        .unwrap_or_else(|| "0000000000000000".to_owned());
    let revision_id = u64::from_str_radix(&revision[..16], 16).unwrap_or(0);

    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    fs::write(
        out.join("build_info.rs"),
        format!(
            "pub const BUILD_REVISION_ID: u64 = {revision_id};\n\
             pub const BUILD_REVISION: &str = \"{}\";\n",
            &revision[..16]
        ),
    )
    .unwrap();
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/index");

    // Put the appropriate `memory.x` in our output directory and ensure it's
    // on the linker search path.
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

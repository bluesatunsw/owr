fn main() {
    cxx_build::bridge("src/lib.rs") // returns a cc::Buil
        .std("c++17")
        .compile("cyphal_vesc_driver_impl");

    println!("cargo:rerun-if-changed=src/lib.cc");
    println!("cargo:rerun-if-changed=include/lib.h");
}

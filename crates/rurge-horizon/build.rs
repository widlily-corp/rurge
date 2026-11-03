//! Build script for rurge-horizon.
//!
//! Increases main thread stack size on Windows (default is 1MB, Horizon requires >=8MB).

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let arg = match std::env::var("CARGO_CFG_TARGET_ENV").as_deref() {
            Ok("msvc") => "/STACK:16777216",
            _ => "-Wl,--stack,16777216",
        };
        println!("cargo:rustc-link-arg-bin=rurge-horizon={arg}");
    }
}

//! Core engine library for the standalone multi-core Rurge client.

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

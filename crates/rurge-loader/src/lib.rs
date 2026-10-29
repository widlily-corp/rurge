//! `rurge-loader` — `.rmod` Archive Extraction & Dynamic Library Loader.
//!
//! Provides archive inspection, host binary extraction, ABI-stable dynamic library
//! loading, and lifecycle coordination for Rurge native mods.

pub mod archive;
pub mod container;
pub mod dylib;

pub use archive::{ArchiveError, RmodArchive};
pub use container::{LoadedPlugin, LoaderError, ModManager};
pub use dylib::{current_target_triple, load_plugin_from_dylib, DylibError, PluginCreateFn};

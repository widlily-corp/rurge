use std::path::Path;
use rurge_api::abi_stable::std_types::RBox;
use rurge_api::RurgePlugin_TO;

/// Errors that can occur when dynamically loading a plugin shared library.
#[derive(Debug, thiserror::Error)]
pub enum DylibError {
    /// Failure loading dynamic library file into the process space.
    #[error("Failed to load dynamic library: {0}")]
    Load(#[from] libloading::Error),

    /// Missing or incompatible entrypoint symbol.
    #[error("Symbol '{symbol}' not found in dynamic library: {source}")]
    SymbolNotFound {
        symbol: &'static str,
        source: libloading::Error,
    },
}

/// Helper function to detect the host platform's Rust target triple at compile-time.
pub fn current_target_triple() -> &'static str {
    #[cfg(all(target_arch = "x86_64", target_os = "windows", target_env = "msvc"))]
    {
        "x86_64-pc-windows-msvc"
    }
    #[cfg(all(target_arch = "x86_64", target_os = "windows", target_env = "gnu"))]
    {
        "x86_64-pc-windows-gnu"
    }
    #[cfg(all(target_arch = "aarch64", target_os = "windows"))]
    {
        "aarch64-pc-windows-msvc"
    }
    #[cfg(all(target_arch = "x86_64", target_os = "linux", target_env = "gnu"))]
    {
        "x86_64-unknown-linux-gnu"
    }
    #[cfg(all(target_arch = "x86_64", target_os = "linux", target_env = "musl"))]
    {
        "x86_64-unknown-linux-musl"
    }
    #[cfg(all(target_arch = "aarch64", target_os = "linux", target_env = "gnu"))]
    {
        "aarch64-unknown-linux-gnu"
    }
    #[cfg(all(target_arch = "aarch64", target_os = "linux", target_env = "musl"))]
    {
        "aarch64-unknown-linux-musl"
    }
    #[cfg(all(target_arch = "x86_64", target_os = "macos"))]
    {
        "x86_64-apple-darwin"
    }
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    {
        "aarch64-apple-darwin"
    }
    #[cfg(not(any(
        all(target_arch = "x86_64", target_os = "windows"),
        all(target_arch = "aarch64", target_os = "windows"),
        all(target_arch = "x86_64", target_os = "linux"),
        all(target_arch = "aarch64", target_os = "linux"),
        all(target_arch = "x86_64", target_os = "macos"),
        all(target_arch = "aarch64", target_os = "macos"),
    )))]
    {
        "unknown-target"
    }
}

/// Function signature for the C-ABI plugin constructor `_rurge_plugin_create`.
pub type PluginCreateFn = unsafe extern "C" fn() -> RurgePlugin_TO<'static, RBox<()>>;

/// Dynamically loads a Rurge plugin from a native shared library file (`.dll`, `.so`, `.dylib`).
///
/// # Safety
/// Loading dynamic libraries and invoking C-ABI entrypoints is inherently `unsafe`.
/// The library must have been built with a compatible ABI and properly export `_rurge_plugin_create`.
pub unsafe fn load_plugin_from_dylib(
    path: &Path,
) -> Result<(RurgePlugin_TO<'static, RBox<()>>, libloading::Library), DylibError> {
    let library = unsafe { libloading::Library::new(path)? };
    let symbol: libloading::Symbol<PluginCreateFn> = unsafe {
        library
            .get(b"_rurge_plugin_create\0")
            .map_err(|source| DylibError::SymbolNotFound {
                symbol: "_rurge_plugin_create",
                source,
            })?
    };
    let plugin = unsafe { symbol() };
    Ok((plugin, library))
}

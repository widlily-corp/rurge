use std::path::{Path, PathBuf};
use rurge_api::abi_stable::std_types::RBox;
use rurge_api::RurgePlugin_TO;

/// Errors that can occur when dynamically loading a plugin shared library.
#[derive(Debug, thiserror::Error)]
pub enum DylibError {
    /// Failure loading dynamic library file into the process space.
    #[error("Failed to load dynamic library: {0}")]
    Load(#[from] libloading::Error),

    /// File I/O failure (e.g. copying library for hot-reload isolation).
    #[error("I/O error during dynamic library preparation: {0}")]
    Io(#[from] std::io::Error),

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

/// Helper function to locate a dynamic library in a directory (`.dll`, `.so`, or `.dylib`).
pub fn find_dylib_in_dir(dir: &Path, filter_name: Option<&str>) -> Option<PathBuf> {
    if !dir.is_dir() {
        return None;
    }
    let is_dylib = |name: &str| {
        let l = name.to_lowercase();
        l.ends_with(".dll") || l.ends_with(".so") || l.ends_with(".dylib")
    };
    if let Ok(entries) = std::fs::read_dir(dir) {
        let mut dylibs = Vec::new();
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_file() {
                let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if is_dylib(fname) {
                    if let Some(target) = filter_name {
                        let normalized_target = target.replace('-', "_");
                        if fname.contains(&normalized_target) {
                            return Some(path);
                        }
                    }
                    dylibs.push(path);
                }
            }
        }
        if dylibs.len() == 1 {
            return Some(dylibs.remove(0));
        }
    }
    None
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

/// Dynamically loads a Rurge plugin by copying the dynamic library to an isolated `dest_path` first.
/// This prevents operating system file locks (especially on Windows) from blocking recompilation
/// during dynamic hot-reloading development workflows.
///
/// # Safety
/// Loading dynamic libraries and invoking C-ABI entrypoints is inherently `unsafe`.
pub unsafe fn load_plugin_from_dylib_copy(
    src_path: &Path,
    dest_path: &Path,
) -> Result<(RurgePlugin_TO<'static, RBox<()>>, libloading::Library), DylibError> {
    if let Some(parent) = dest_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(src_path, dest_path)?;
    unsafe { load_plugin_from_dylib(dest_path) }
}

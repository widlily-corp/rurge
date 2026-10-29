use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use rurge_api::abi_stable::std_types::RBox;
use rurge_api::{ModMetadata, RurgePlugin_TO};
use rurge_blamer::Blamer;
use rurge_core::{DependencyResolver, ModManifest, ResolverError};

use crate::archive::{ArchiveError, RmodArchive};
use crate::dylib::{current_target_triple, load_plugin_from_dylib, DylibError};

/// Errors that can occur during mod loading and orchestration.
#[derive(Debug, thiserror::Error)]
pub enum LoaderError {
    /// File system or I/O failure.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Archive opening, decompression or extraction failure.
    #[error("Archive error: {0}")]
    Archive(#[from] ArchiveError),

    /// Dependency resolution or topological sorting failure.
    #[error("Dependency resolution failed: {0}")]
    Resolver(#[from] ResolverError),

    /// Dynamic library loading or entrypoint invocation failure.
    #[error("Dynamic library error: {0}")]
    Dylib(#[from] DylibError),
}

/// Represents an active, dynamically loaded mod plugin in memory.
pub struct LoadedPlugin {
    /// Manifest parsed from the `.rmod` archive (`rurge.toml`).
    pub manifest: ModManifest,

    /// ABI-stable metadata reported by the plugin.
    pub metadata: ModMetadata,

    /// Mutex-wrapped ABI-stable plugin trait object.
    pub plugin: parking_lot::Mutex<RurgePlugin_TO<'static, RBox<()>>>,

    /// Loaded dynamic library handle (kept alive so symbols remain mapped).
    pub _library: libloading::Library,
}

/// Orchestrator responsible for discovering `.rmod` archives, resolving dependencies,
/// extracting native libraries, and managing plugin lifecycles.
pub struct ModManager {
    blamer: Arc<Blamer>,
    loaded_plugins: Vec<Arc<LoadedPlugin>>,
}

impl ModManager {
    /// Creates a new `ModManager` linked to the given crash diagnostic blamer.
    pub fn new(blamer: Arc<Blamer>) -> Self {
        Self {
            blamer,
            loaded_plugins: Vec::new(),
        }
    }

    /// Returns a reference to the crash blamer.
    pub fn blamer(&self) -> &Arc<Blamer> {
        &self.blamer
    }

    /// Returns a slice of all currently loaded mod plugins.
    pub fn loaded_plugins(&self) -> &[Arc<LoadedPlugin>] {
        &self.loaded_plugins
    }

    /// Scans `mods_dir` for files ending with `.rmod` and opens them.
    pub fn discover_archives(&self, mods_dir: &Path) -> Result<Vec<RmodArchive>, LoaderError> {
        let mut archives = Vec::new();
        if !mods_dir.exists() {
            return Ok(archives);
        }
        for entry in std::fs::read_dir(mods_dir)? {
            let entry = entry?;
            let path = entry.path();
            let is_rmod = path.is_file()
                && path
                    .extension()
                    .and_then(|s| s.to_str())
                    .map(|s| s.eq_ignore_ascii_case("rmod"))
                    .unwrap_or(false);
            if is_rmod {
                let archive = RmodArchive::open(&path)?;
                archives.push(archive);
            }
        }
        Ok(archives)
    }

    /// Reads manifests from archives and performs DAG dependency resolution.
    pub fn resolve_archives(&self, archives: &[RmodArchive]) -> Result<Vec<ModManifest>, LoaderError> {
        let mut manifests = Vec::with_capacity(archives.len());
        for archive in archives {
            manifests.push(archive.read_manifest()?);
        }
        let resolved = DependencyResolver::resolve(manifests)?;
        Ok(resolved)
    }

    /// Discovers archives in `mods_dir` and resolves their manifests in topological order.
    pub fn resolve_mods(&self, mods_dir: &Path) -> Result<Vec<ModManifest>, LoaderError> {
        let archives = self.discover_archives(mods_dir)?;
        self.resolve_archives(&archives)
    }

    /// Discovers archives, extracts host binaries to `cache_dir`, loads dylibs in dependency order,
    /// and registers them.
    pub fn load_mods(
        &mut self,
        mods_dir: &Path,
        cache_dir: &Path,
    ) -> Result<Vec<Arc<LoadedPlugin>>, LoaderError> {
        let archives = self.discover_archives(mods_dir)?;
        self.load_from_archives(&archives, cache_dir)
    }

    /// Loads plugins from a pre-discovered slice of archives in dependency order.
    pub fn load_from_archives(
        &mut self,
        archives: &[RmodArchive],
        cache_dir: &Path,
    ) -> Result<Vec<Arc<LoadedPlugin>>, LoaderError> {
        if archives.is_empty() {
            return Ok(Vec::new());
        }

        // 1. Read manifests and map to archives.
        let mut manifest_map: HashMap<String, &RmodArchive> = HashMap::with_capacity(archives.len());
        let mut manifests = Vec::with_capacity(archives.len());

        for archive in archives {
            let manifest = archive.read_manifest()?;
            manifest_map.insert(manifest.id.clone(), archive);
            manifests.push(manifest);
        }

        // 2. Resolve DAG topological ordering.
        let sorted_manifests = DependencyResolver::resolve(manifests)?;

        // 3. Extract and load each plugin in dependency order.
        let target_triple = current_target_triple();
        let mut newly_loaded = Vec::with_capacity(sorted_manifests.len());

        for manifest in sorted_manifests {
            let archive = manifest_map
                .get(&manifest.id)
                .expect("Archive must exist for resolved manifest");
            let mod_cache_dir = cache_dir.join(&manifest.id);

            // Extract binary
            let binary_path = archive.extract_binary(target_triple, &mod_cache_dir)?;

            // Extract assets (if present)
            let _ = archive.extract_assets(&mod_cache_dir);

            // Dynamically load library and instantiate plugin
            let (plugin_to, library) = unsafe { load_plugin_from_dylib(&binary_path)? };
            let metadata = plugin_to.metadata();

            let loaded = Arc::new(LoadedPlugin {
                manifest,
                metadata,
                plugin: parking_lot::Mutex::new(plugin_to),
                _library: library,
            });

            self.loaded_plugins.push(Arc::clone(&loaded));
            newly_loaded.push(loaded);
        }

        Ok(newly_loaded)
    }

    /// Shuts down all loaded plugins in reverse topological order inside panic boundaries.
    pub fn shutdown_all(&mut self) {
        for loaded in self.loaded_plugins.iter().rev() {
            let mod_id = loaded.manifest.id.clone();
            let plugin_arc = Arc::clone(loaded);
            let _ = rurge_blamer::safe_invoke(
                &mod_id,
                "on_shutdown",
                &self.blamer,
                std::panic::AssertUnwindSafe(move || {
                    let mut plugin_lock = plugin_arc.plugin.lock();
                    plugin_lock.on_shutdown();
                }),
            );
        }
        self.loaded_plugins.clear();
    }
}

impl Drop for ModManager {
    fn drop(&mut self) {
        self.shutdown_all();
    }
}

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use rurge_api::abi_stable::std_types::RBox;
use rurge_api::{ModMetadata, RurgePlugin_TO};
use rurge_blamer::Blamer;
use rurge_core::{DependencyResolver, ModManifest, ResolverError};

use crate::archive::{ArchiveError, RmodArchive};
use crate::dylib::{current_target_triple, find_dylib_in_dir, load_plugin_from_dylib, DylibError};

/// Errors that can occur during mod loading, orchestration, or hot reloading.
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

    /// Specified mod identifier was not found in loaded or discovered plugins.
    #[error("Mod '{0}' not found")]
    ModNotFound(String),
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

impl std::fmt::Debug for LoadedPlugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoadedPlugin")
            .field("manifest", &self.manifest)
            .field("metadata", &self.metadata)
            .finish()
    }
}

/// Orchestrator responsible for discovering `.rmod` archives, resolving dependencies,
/// extracting native libraries, and managing plugin lifecycles including dynamic hot reloading.
pub struct ModManager {
    blamer: Arc<Blamer>,
    loaded_plugins: Vec<Arc<LoadedPlugin>>,
    archive_paths: HashMap<String, PathBuf>,
    dylib_paths: HashMap<String, PathBuf>,
    reload_counters: HashMap<String, usize>,
}

impl ModManager {
    /// Creates a new `ModManager` linked to the given crash diagnostic blamer.
    pub fn new(blamer: Arc<Blamer>) -> Self {
        Self {
            blamer,
            loaded_plugins: Vec::new(),
            archive_paths: HashMap::new(),
            dylib_paths: HashMap::new(),
            reload_counters: HashMap::new(),
        }
    }

    /// Returns a reference to the crash blamer.
    pub fn blamer(&self) -> &Arc<Blamer> {
        &self.blamer
    }

    /// Returns a vector of all currently loaded mod plugins.
    pub fn loaded_plugins(&self) -> Vec<Arc<LoadedPlugin>> {
        self.loaded_plugins.clone()
    }

    /// Retrieves an active loaded plugin by its mod identifier.
    pub fn get_plugin(&self, mod_id: &str) -> Option<Arc<LoadedPlugin>> {
        self.loaded_plugins
            .iter()
            .find(|p| p.manifest.id == mod_id)
            .cloned()
    }

    /// Registers a pre-instantiated plugin into the manager (useful for direct testing and hot reloads).
    pub fn register_loaded_plugin(
        &mut self,
        loaded: Arc<LoadedPlugin>,
        archive_path: Option<PathBuf>,
        dylib_path: Option<PathBuf>,
    ) {
        let id = loaded.manifest.id.clone();
        if let Some(pos) = self.loaded_plugins.iter().position(|p| p.manifest.id == id) {
            self.loaded_plugins[pos] = Arc::clone(&loaded);
        } else {
            self.loaded_plugins.push(Arc::clone(&loaded));
        }
        if let Some(ap) = archive_path {
            self.archive_paths.insert(id.clone(), ap);
        }
        if let Some(dp) = dylib_path {
            self.dylib_paths.insert(id, dp);
        }
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

            // Extract assets (if present)
            let _ = archive.extract_assets(&mod_cache_dir);

            // Purely data-driven mods containing declarative content.json without a dynamic library
            // do not require binary extraction or dylib loading.
            if !archive.has_binary(target_triple) && archive.has_content_schema() {
                continue;
            }

            // Extract binary
            let binary_path = archive.extract_binary(target_triple, &mod_cache_dir)?;

            // Dynamically load library and instantiate plugin
            let (plugin_to, library) = unsafe { load_plugin_from_dylib(&binary_path)? };
            let metadata = plugin_to.metadata();

            let loaded = Arc::new(LoadedPlugin {
                manifest: manifest.clone(),
                metadata,
                plugin: parking_lot::Mutex::new(plugin_to),
                _library: library,
            });

            self.archive_paths.insert(manifest.id.clone(), archive.path().to_path_buf());
            self.dylib_paths.insert(manifest.id.clone(), binary_path);
            self.loaded_plugins.push(Arc::clone(&loaded));
            newly_loaded.push(loaded);
        }

        Ok(newly_loaded)
    }

    /// Unloads a loaded mod plugin by its mod ID, invoking its `on_shutdown` within safe panic boundaries,
    /// dropping the dynamic library handle, and removing it from the active plugin list.
    pub fn unload_plugin(&mut self, mod_id: &str) -> Result<Arc<LoadedPlugin>, LoaderError> {
        let pos = self
            .loaded_plugins
            .iter()
            .position(|p| p.manifest.id == mod_id)
            .ok_or_else(|| LoaderError::ModNotFound(mod_id.to_string()))?;

        let old_plugin = self.loaded_plugins.remove(pos);
        let plugin_arc = Arc::clone(&old_plugin);
        let _ = rurge_blamer::safe_invoke(
            mod_id,
            "on_shutdown",
            &self.blamer,
            std::panic::AssertUnwindSafe(move || {
                let mut plugin_lock = plugin_arc.plugin.lock();
                plugin_lock.on_shutdown();
            }),
        );

        Ok(old_plugin)
    }

    /// Dynamically unloads and reloads a plugin library (`.dll` / `.so` / `.dylib`),
    /// calling `on_shutdown` on the old instance, releasing OS file locks, extracting or copying
    /// the updated binary into `cache_dir`, dynamically linking it, and updating the manager.
    pub fn reload_plugin(
        &mut self,
        mod_id: &str,
        cache_dir: &Path,
    ) -> Result<Arc<LoadedPlugin>, LoaderError> {
        let pos = self
            .loaded_plugins
            .iter()
            .position(|p| p.manifest.id == mod_id)
            .ok_or_else(|| LoaderError::ModNotFound(mod_id.to_string()))?;

        let old_manifest = self.loaded_plugins[pos].manifest.clone();

        // 1. Unload old plugin and execute on_shutdown inside panic boundaries
        let old_plugin = self.loaded_plugins.remove(pos);
        let plugin_arc = Arc::clone(&old_plugin);
        let _ = rurge_blamer::safe_invoke(
            mod_id,
            "on_shutdown",
            &self.blamer,
            std::panic::AssertUnwindSafe(move || {
                let mut plugin_lock = plugin_arc.plugin.lock();
                plugin_lock.on_shutdown();
            }),
        );

        // Explicitly drop old plugin to release OS file locks
        drop(old_plugin);

        let counter = self.reload_counters.entry(mod_id.to_string()).or_insert(0);
        *counter += 1;
        let reload_gen = *counter;

        // 2. Discover archive or direct dylib source
        let target_triple = current_target_triple();
        let mod_cache_dir = cache_dir.join(format!("{mod_id}_reload_{reload_gen}"));
        std::fs::create_dir_all(&mod_cache_dir)?;

        let registered_archive = self.archive_paths.get(mod_id).cloned();
        let candidate_archive = registered_archive.filter(|p| p.is_file()).or_else(|| {
            let p1 = cache_dir.join(format!("{mod_id}.rmod"));
            if p1.is_file() {
                Some(p1)
            } else {
                let p2 = mod_cache_dir.join(format!("{mod_id}.rmod"));
                if p2.is_file() {
                    Some(p2)
                } else {
                    None
                }
            }
        });

        if let Some(archive_path) = candidate_archive {
            let archive = RmodArchive::open(&archive_path)?;
            let manifest = archive.read_manifest()?;
            let _ = archive.extract_assets(&mod_cache_dir);

            let extracted_bin = archive.extract_binary(target_triple, &mod_cache_dir)?;
            let ext = extracted_bin
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("dll");

            // Copy to generation-isolated file to avoid OS caching / locks
            let versioned_bin = mod_cache_dir.join(format!("{mod_id}_reload_{reload_gen}.{ext}"));
            let bin_to_load = if std::fs::copy(&extracted_bin, &versioned_bin).is_ok() {
                versioned_bin
            } else {
                extracted_bin
            };

            let (plugin_to, library) = unsafe { load_plugin_from_dylib(&bin_to_load)? };
            let metadata = plugin_to.metadata();

            let loaded = Arc::new(LoadedPlugin {
                manifest,
                metadata,
                plugin: parking_lot::Mutex::new(plugin_to),
                _library: library,
            });

            self.archive_paths.insert(mod_id.to_string(), archive_path);
            self.dylib_paths.insert(mod_id.to_string(), bin_to_load);
            self.loaded_plugins.insert(pos, Arc::clone(&loaded));

            return Ok(loaded);
        }

        // Direct dynamic library reload (without .rmod archive)
        let candidate_dylib = self
            .dylib_paths
            .get(mod_id)
            .filter(|p| p.is_file())
            .cloned()
            .or_else(|| {
                find_dylib_in_dir(&mod_cache_dir, Some(mod_id))
                    .or_else(|| find_dylib_in_dir(cache_dir, Some(mod_id)))
            });

        let src_dylib = candidate_dylib.ok_or_else(|| {
            LoaderError::ModNotFound(format!(
                "No archive or dynamic library found to reload mod '{mod_id}'"
            ))
        })?;

        let manifest_candidate = src_dylib
            .parent()
            .map(|p| p.join("rurge.toml"))
            .filter(|p| p.is_file())
            .or_else(|| {
                let p = mod_cache_dir.join("rurge.toml");
                if p.is_file() {
                    Some(p)
                } else {
                    None
                }
            });

        let manifest = if let Some(m_path) = manifest_candidate {
            ModManifest::from_file(&m_path).unwrap_or(old_manifest)
        } else {
            old_manifest
        };

        let ext = src_dylib
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("dll");
        let versioned_bin = mod_cache_dir.join(format!("{mod_id}_reload_{reload_gen}.{ext}"));
        let bin_to_load = if std::fs::copy(&src_dylib, &versioned_bin).is_ok() {
            versioned_bin
        } else {
            src_dylib.clone()
        };

        let (plugin_to, library) = unsafe { load_plugin_from_dylib(&bin_to_load)? };
        let metadata = plugin_to.metadata();

        let loaded = Arc::new(LoadedPlugin {
            manifest,
            metadata,
            plugin: parking_lot::Mutex::new(plugin_to),
            _library: library,
        });

        self.dylib_paths.insert(mod_id.to_string(), bin_to_load);
        self.loaded_plugins.insert(pos, Arc::clone(&loaded));

        Ok(loaded)
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
        self.archive_paths.clear();
        self.dylib_paths.clear();
        self.reload_counters.clear();
    }
}

impl Drop for ModManager {
    fn drop(&mut self) {
        self.shutdown_all();
    }
}

//! Engine host lifecycle managing configuration, multi-core hardware budgeting,
//! sound asset linking, shaderpack activation, mod conversion/loading, and client execution.

use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Configuration parameters for the Rurge standalone engine host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineConfig {
    /// Root game directory where assets, options, and logs reside.
    pub game_dir: PathBuf,
    /// Directory containing `.rmod` packages and dropped `.jar` mods.
    pub mods_dir: PathBuf,
    /// Directory containing OptiFine/Iris compatible shaderpacks.
    pub shaderpacks_dir: PathBuf,
    /// Target frame rate cap when active and focused.
    pub target_fps: u32,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            game_dir: PathBuf::from("."),
            mods_dir: PathBuf::from("mods"),
            shaderpacks_dir: PathBuf::from("shaderpacks"),
            target_fps: 144,
        }
    }
}

impl EngineConfig {
    /// Creates a default engine configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the base game directory.
    pub fn with_game_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.game_dir = path.into();
        self
    }

    /// Sets the mods directory.
    pub fn with_mods_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.mods_dir = path.into();
        self
    }

    /// Sets the shaderpacks directory.
    pub fn with_shaderpacks_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.shaderpacks_dir = path.into();
        self
    }

    /// Sets the target frame rate.
    pub fn with_target_fps(mut self, fps: u32) -> Self {
        self.target_fps = fps;
        self
    }

    /// Preflight validation ensuring critical directory structure exists.
    pub fn preflight(&self) -> Result<(), Box<dyn std::error::Error>> {
        std::fs::create_dir_all(&self.game_dir)?;
        std::fs::create_dir_all(&self.mods_dir)?;
        std::fs::create_dir_all(&self.shaderpacks_dir)?;
        Ok(())
    }
}

/// Launches the full Rurge voxel game client with default configuration.
pub fn launch() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    launch_with_args(&args)
}

/// Launches the full Rurge voxel game client using command line arguments.
pub fn launch_with_args(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let mut config = EngineConfig::default();

    let mut iter = args.iter().peekable();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--game-dir" | "--gameDir" => {
                if let Some(val) = iter.next() {
                    config.game_dir = PathBuf::from(val);
                    config.mods_dir = config.game_dir.join("mods");
                    config.shaderpacks_dir = config.game_dir.join("shaderpacks");
                }
            }
            "--mods-dir" => {
                if let Some(val) = iter.next() {
                    config.mods_dir = PathBuf::from(val);
                }
            }
            "--shaderpacks-dir" => {
                if let Some(val) = iter.next() {
                    config.shaderpacks_dir = PathBuf::from(val);
                }
            }
            "--fps" | "--target-fps" => {
                if let Some(val) = iter.next()
                    && let Ok(fps) = val.parse::<u32>() {
                        config.target_fps = fps;
                    }
            }
            _ => {}
        }
    }

    launch_with_config(&config)
}

/// Executes the complete engine host lifecycle with the supplied configuration.
pub fn launch_with_config(config: &EngineConfig) -> Result<(), Box<dyn std::error::Error>> {
    // a. Initialize engine branding and window titles
    crate::branding::init_engine_branding(crate::version());

    log::info!("========================================================");
    log::info!("         Rurge Engine Host v{} [144+ FPS]               ", crate::version());
    log::info!("========================================================");

    // Preflight environment
    config.preflight()?;

    // b. Scan CPU topology (C_game = C_phys - 2) and initialize global Rayon pool
    let phys_cores = num_cpus::get_physical();
    let log_threads = num_cpus::get();
    let cpu_budget = crate::hardware::compute_cpu_budget(phys_cores, log_threads);

    let _ = rayon::ThreadPoolBuilder::new()
        .num_threads(cpu_budget.worker_threads)
        .thread_name(|i| format!("rurge-worker-{i}"))
        .build_global();

    // c. Scan RAM budget and configure memory ceilings
    let mut sys = sysinfo::System::new_all();
    sys.refresh_memory();
    let ram_budget = crate::hardware::compute_ram_budget(sys.total_memory());

    log::info!(
        "CPU Topology: {} phys cores, {} log threads -> Budget: {} game cores ({} OS reserved), {} Rayon worker threads",
        cpu_budget.total_physical_cores,
        cpu_budget.total_logical_threads,
        cpu_budget.game_physical_cores,
        cpu_budget.os_reserved_cores,
        cpu_budget.worker_threads
    );
    log::info!(
        "Memory Tier: {:?} -> Max Game RAM: {:.2} GB, Chunk Mesh Radius: {}",
        ram_budget.tier,
        ram_budget.max_game_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
        ram_budget.max_chunk_mesh_radius
    );

    // d. Link sound assets from virtual index
    match crate::sound_linker::ensure_sound_assets_linked(&config.game_dir) {
        Ok(count) => log::info!("Sound assets linked successfully ({count} objects)"),
        Err(e) => log::warn!("Sound asset linking notice: {e}"),
    }

    // e. Ensure default shaderpack installed and configure live shaderpack
    match crate::shaderpack_bundle::ensure_default_shaderpack_installed(&config.shaderpacks_dir) {
        Ok(path) => log::info!("Default shaderpack verified at '{}'", path.display()),
        Err(e) => log::warn!("Default shaderpack installation notice: {e}"),
    }
    ensure_compatible_graphics_config(&config.game_dir);

    let comp_sp = config.shaderpacks_dir.join("ComplementaryReimagined_r5.9.3.zip");
    let comp_sp_old = config.shaderpacks_dir.join("ComplementaryReimagined_r5.8.1.zip");
    let best_sp = if comp_sp.exists() {
        "ComplementaryReimagined_r5.9.3.zip"
    } else if comp_sp_old.exists() {
        "ComplementaryReimagined_r5.8.1.zip"
    } else {
        "test-classic-shader.zip"
    };
    horizon_shaders::set_live_shader_pack(best_sp);
    log::info!("Live shaderpack active: {best_sp}");

    // f. Convert dropped .jar mods in mods_dir to .rmod packages
    let converted = crate::jar_scanner::convert_dropped_jars_in(&config.mods_dir);
    if !converted.is_empty() {
        log::info!("Auto-converted {} dropped JAR mod(s) to native .rmod", converted.len());
    }
    sync_hmod_packages(&config.mods_dir);
    ensure_mods_config(&[&config.mods_dir]);

    // g. Boot RurgeEngine and load .rmod plugins
    let mut engine = rurge_runtime::engine::RurgeEngine::new();
    if let Err(e) = engine.initialize() {
        log::error!("Failed to initialize RurgeEngine: {e}");
    } else {
        if let Ok(archives) = engine.mod_manager().discover_archives(&config.mods_dir) {
            for archive in &archives {
                if let Ok(Some(schema_str)) = archive.read_content_schema()
                    && let Ok(schema) = rurge_converter::ContentSchema::from_json_str(&schema_str)
                {
                    let _ = engine.content().register_schema(&schema.mod_id, &schema);
                }
            }
        }
        let loaded_count = engine.mod_manager().loaded_plugins().len();
        let content_count = engine.content().block_count() + engine.content().item_count();
        log::info!("Discovered and loaded {loaded_count} .rmod plugin(s), {content_count} custom content item(s)");
    }

    // h. Package mod hooks into Mods trait collection
    let engine = Arc::new(engine);
    let hooks = crate::hooks::RurgeHorizonHooks::new(Arc::clone(&engine));
    let mods: horizon_client::mod_api::Mods = Arc::new([Box::new(hooks)]);

    // i. Launch client via horizon_client::entry::main_with
    log::info!("Launching Rurge voxel client (fork of Horizon engine)...");
    horizon_client::entry::main_with(mods);

    // j. Clean shutdown of RurgeEngine on exit
    log::info!("Rurge client terminated. Shutting down RurgeEngine...");
    engine.shutdown();
    log::info!("Rurge Engine Host shutdown cleanly.");

    Ok(())
}

/// Ensures that every `.rmod` package has a twin `.hmod` package so Horizon mounts its assets and data.
fn sync_hmod_packages(mods_dir: &Path) {
    if let Ok(entries) = std::fs::read_dir(mods_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() && p.extension().is_some_and(|e| e == "rmod") {
                let hmod = p.with_extension("hmod");
                let should_copy = if !hmod.exists() {
                    true
                } else {
                    let rmod_meta = std::fs::metadata(&p);
                    let hmod_meta = std::fs::metadata(&hmod);
                    match (rmod_meta, hmod_meta) {
                        (Ok(rm), Ok(hm)) => {
                            rm.len() != hm.len()
                                || rm.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH)
                                    > hm.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH)
                        }
                        _ => true,
                    }
                };
                if should_copy {
                    let _ = std::fs::copy(&p, &hmod);
                    // Invalidate stamp so Horizon recompiles mod-content
                    let stamp = Path::new(".horizon-cache/mod-content/horizon-mod-content.stamp");
                    if stamp.exists() {
                        let _ = std::fs::remove_file(stamp);
                    }
                }
            }
        }
    }
}

/// Ensures that all detected mods are marked enabled in `horizon-mods.txt`.
fn ensure_mods_config(dirs: &[&Path]) {
    let config_path = Path::new("horizon-mods.txt");
    let content = std::fs::read_to_string(config_path).unwrap_or_default();
    let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
    let mut modified = false;

    let mut detected_ids = Vec::new();
    for dir in dirs {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                let is_pkg = p.is_file()
                    && p.extension().is_some_and(|e| e == "rmod" || e == "hmod");
                if is_pkg
                    && let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                        let id = stem.split('-').next().unwrap_or(stem).to_string();
                        if !detected_ids.contains(&id) {
                            detected_ids.push(id);
                        }
                    }
            }
        }
    }

    for id in detected_ids {
        let expected = format!("mod.{id}:true");
        let disabled_pattern = format!("mod.{id}:false");
        let native_pattern = format!("mod.rurge_mod_{id}:false");

        for line in &mut lines {
            if *line == disabled_pattern || *line == native_pattern {
                *line = expected.clone();
                modified = true;
            }
        }

        if !lines.iter().any(|l| l.trim() == expected) {
            lines.push(expected);
            modified = true;
        }
    }

    if modified {
        let _ = std::fs::write(config_path, lines.join("\n") + "\n");
    }
}

/// Activates shaderpack in `horizon-graphics.txt` and ensures modern graphics backend is set.
fn ensure_compatible_graphics_config(game_dir: &Path) {
    let graphics_path = game_dir.join("horizon-graphics.txt");
    let content = std::fs::read_to_string(&graphics_path).unwrap_or_default();
    let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
    let mut modified = false;

    let best_shader = if game_dir.join("shaderpacks/ComplementaryReimagined_r5.9.3.zip").exists() {
        "ComplementaryReimagined_r5.9.3.zip"
    } else if game_dir.join("shaderpacks/ComplementaryReimagined_r5.8.1.zip").exists() {
        "ComplementaryReimagined_r5.8.1.zip"
    } else {
        "test-classic-shader.zip"
    };

    let has_picked = lines.iter().any(|l| {
        l.starts_with("pickedShaderPack:")
            && !l.trim_start_matches("pickedShaderPack:").trim().is_empty()
            && l.trim_start_matches("pickedShaderPack:").trim() != "test-classic-shader.zip"
    });
    if !has_picked {
        let mut found = false;
        for line in &mut lines {
            if line.starts_with("pickedShaderPack:") {
                *line = format!("pickedShaderPack:{best_shader}");
                found = true;
                break;
            }
        }
        if !found {
            lines.push(format!("pickedShaderPack:{best_shader}"));
        }
        modified = true;
    }

    let mut api_found = false;
    for line in &mut lines {
        if line.starts_with("graphicsApi:") {
            api_found = true;
            if line.trim() == "graphicsApi:opengl" {
                *line = "graphicsApi:auto".to_string();
                modified = true;
            }
            break;
        }
    }
    if !api_found {
        lines.push("graphicsApi:auto".to_string());
        modified = true;
    }

    if modified {
        let _ = std::fs::write(&graphics_path, lines.join("\n") + "\n");
    }

    let options_path = game_dir.join("options.txt");
    if let Ok(opt_content) = std::fs::read_to_string(&options_path) {
        let mut opt_lines: Vec<String> = opt_content.lines().map(|s| s.to_string()).collect();
        let mut opt_modified = false;
        for line in &mut opt_lines {
            if line.starts_with("preferredGraphicsBackend:") && line.contains("opengl") {
                *line = "preferredGraphicsBackend:\"default\"".to_string();
                opt_modified = true;
            }
            if line.starts_with("exclusiveFullscreen:") && line.contains("true") {
                *line = "exclusiveFullscreen:false".to_string();
                opt_modified = true;
            }
            if line.starts_with("fullscreen:") && line.contains("true") {
                *line = "fullscreen:false".to_string();
                opt_modified = true;
            }
        }
        if opt_modified {
            let _ = std::fs::write(&options_path, opt_lines.join("\n") + "\n");
        }
    }
}

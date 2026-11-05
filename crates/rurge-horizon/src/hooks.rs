use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use abi_stable::std_types::RString;
use horizon_client::mod_api::{
    BlocksEvent, CommandEvent, CommandsEvent, Corner, Flow, Hooks, Host, HudEvent, ItemsEvent,
    KeyEvent, ModsEvent, NativeMod, SettingsClickEvent, SettingsEvent, ShaderParamsEvent, Text,
};
use horizon_mods::content::{Block as HorizonBlock, Item as HorizonItem};
use horizon_client_render::gfx::native_render::{
    wgpu, HandDrawnEvent, RenderHooks, RenderNeedsEvent, WorldDrawnEvent,
};
use parking_lot::RwLock;
use rurge_api::events::{RenderPassContext, ShaderpackLoadedEvent, ShaderpackUnloadedEvent};
use rurge_runtime::engine::RurgeEngine;

/// Native bridge linking Horizon's mod hooks and render pipeline to `RurgeEngine`.
pub struct RurgeHorizonHooks {
    /// Shared reference to the central Rurge runtime engine.
    pub engine: Arc<RurgeEngine>,
    /// Track currently active shader pack to detect and dispatch transition events.
    current_shader: RwLock<Option<String>>,
}

impl RurgeHorizonHooks {
    /// Creates a new native hooks bridge wrapping the shared runtime engine.
    pub fn new(engine: Arc<RurgeEngine>) -> Self {
        let initial_shader = Self::query_active_shaderpack();
        Self {
            engine,
            current_shader: RwLock::new(initial_shader),
        }
    }

    /// Safe hook point where mods can set uniform data.
    pub fn shader_params(&self, params: &mut [[f32; 4]; 8]) {
        *params = self.engine.get_shader_uniforms();
    }

    /// Returns the currently tracked active shaderpack name, if any.
    pub fn current_shader(&self) -> Option<String> {
        let val = self.current_shader.read().clone();
        if val.is_none() {
            if let Some(active) = Self::query_active_shaderpack() {
                *self.current_shader.write() = Some(active.clone());
                return Some(active);
            }
        }
        val
    }

    /// Queries the currently active shaderpack name from `horizon-graphics.txt` in the specified directory.
    pub fn query_active_shaderpack_in(dir: &Path) -> Option<String> {
        let path = dir.join("horizon-graphics.txt");
        if let Ok(content) = std::fs::read_to_string(path) {
            let g = horizon_client_render::graphics::Graphics::parse(&content);
            if !g.shader_pack.is_empty() {
                return Some(g.shader_pack);
            }
        }
        None
    }

    /// Queries the currently active shaderpack name from `horizon-graphics.txt` in the current working directory.
    pub fn query_active_shaderpack() -> Option<String> {
        Self::query_active_shaderpack_in(Path::new("."))
    }

    /// Checks if active shaderpack changed and dispatches `ShaderpackLoadedEvent` or `ShaderpackUnloadedEvent`.
    pub fn check_and_dispatch_shader_transition(&self, new_pack: Option<String>) {
        let mut current = self.current_shader.write();
        if *current != new_pack {
            match &new_pack {
                Some(name) => {
                    let event = ShaderpackLoadedEvent {
                        pack_name: RString::from(name.as_str()),
                        is_classic: true,
                    };
                    self.engine.event_bus().dispatch(&event);
                }
                None => {
                    let event = ShaderpackUnloadedEvent;
                    self.engine.event_bus().dispatch(&event);
                }
            }
            *current = new_pack;
        }
    }

    /// Toggles between the default shaderpack `test-classic-shader.zip` and none in the specified directory,
    /// persisting the choice in `horizon-graphics.txt`.
    pub fn toggle_default_shaderpack_in(&self, dir: &Path) {
        let path = dir.join("horizon-graphics.txt");
        let current = Self::query_active_shaderpack_in(dir);

        // Discover all shaderpack .zip files in shaderpacks/ directory
        let sp_dir = dir.join("shaderpacks");
        let mut available: Vec<String> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&sp_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() && p.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("zip")) {
                    if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                        available.push(name.to_string());
                    }
                }
            }
        }
        available.sort();
        if available.is_empty() {
            available.push("test-classic-shader.zip".to_string());
        }

        let new_val = match &current {
            None => available.first().map(|s| s.as_str()).unwrap_or(""),
            Some(cur) => {
                if let Some(idx) = available.iter().position(|s| s == cur) {
                    if idx + 1 < available.len() {
                        available[idx + 1].as_str()
                    } else {
                        ""
                    }
                } else {
                    available.first().map(|s| s.as_str()).unwrap_or("")
                }
            }
        };

        let content = std::fs::read_to_string(&path).unwrap_or_default();
        let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
        let mut replaced = false;
        for line in &mut lines {
            if line.starts_with("pickedShaderPack:") {
                *line = format!("pickedShaderPack:{new_val}");
                replaced = true;
                break;
            }
        }
        if !replaced {
            lines.push(format!("pickedShaderPack:{new_val}"));
        }
        // When enabling shaders, ensure graphicsApi is not legacy opengl (which breaks depth textureLoad)
        if !new_val.is_empty() {
            for line in &mut lines {
                if line.starts_with("graphicsApi:") && line.trim() == "graphicsApi:opengl" {
                    *line = "graphicsApi:auto".to_string();
                }
            }
        }
        let _ = std::fs::write(&path, lines.join("\n") + "\n");
        let new_opt = if new_val.is_empty() {
            None
        } else {
            Some(new_val.to_string())
        };
        self.check_and_dispatch_shader_transition(new_opt);
    }

    /// Toggles between the default shaderpack `test-classic-shader.zip` and none,
    /// persisting the choice in `horizon-graphics.txt` in the current directory.
    pub fn toggle_default_shaderpack(&self) {
        self.toggle_default_shaderpack_in(Path::new("."));
    }

    /// Populates the list of native Rurge mods and shader info for the UI.
    pub fn populate_native_mods(&self, mods: &mut Vec<NativeMod>) {
        let loaded_plugins = self.engine.mod_manager().loaded_plugins();
        let loaded_count = loaded_plugins.len();
        let active_pack = self.current_shader().unwrap_or_else(|| "None (Vanilla)".to_string());
        let content = self.engine.content();
        let custom_blocks = content.block_count();
        let custom_items = content.item_count();

        // 1. Rurge Core Mod Loader entry
        let core_desc = if custom_blocks > 0 || custom_items > 0 {
            format!(
                "High-performance native C-ABI mod loader. Loaded: {} .rmod mod(s), {} custom blocks/items.",
                loaded_count,
                custom_blocks + custom_items
            )
        } else {
            format!(
                "High-performance native C-ABI mod loader. Loaded: {} .rmod mod(s).",
                loaded_count
            )
        };

        mods.push(NativeMod {
            id: "rurge_core".to_string(),
            title: "Rurge Mod Loader 0.2.0 [Rust]".to_string(),
            description: core_desc,
            keys: vec![],
        });

        // 2. Rurge Shaders entry
        mods.push(NativeMod {
            id: "rurge_shaders".to_string(),
            title: format!("✨ Shaders: {}", active_pack),
            description: "OptiFine & Iris Classic Shaders support (.zip packs in shaderpacks/)"
                .to_string(),
            keys: vec![[
                "key_toggle_shaders".to_string(),
                "Toggle Shaders (K)".to_string(),
                "key.keyboard.k".to_string(),
            ]],
        });

        // 3. Registered .rmod plugins (native dynamic libraries)
        let mut listed_mod_ids = std::collections::HashSet::new();
        for plugin in &loaded_plugins {
            let meta = &plugin.metadata;
            let mod_id = meta.id.as_str();
            listed_mod_ids.insert(mod_id.to_string());
            let authors: Vec<&str> = meta.authors.iter().map(|a| a.as_str()).collect();

            let b_count = content.blocks_for_mod(mod_id).len();
            let i_count = content.items_for_mod(mod_id).len();
            let is_converted = b_count > 0 || i_count > 0;

            let title = if is_converted {
                format!("[Converted JAR] {} v{} [.rmod]", meta.name.as_str(), meta.version.as_str())
            } else {
                format!("{} v{} [.rmod]", meta.name.as_str(), meta.version.as_str())
            };

            let mut desc = format!("{} — By {}", meta.description.as_str(), authors.join(", "));
            if is_converted {
                desc = format!("{} (Blocks: {}, Items: {})", desc, b_count, i_count);
            }

            mods.push(NativeMod {
                id: format!("rurge_mod_{}", mod_id),
                title,
                description: desc,
                keys: vec![],
            });
        }

        // 4. Converted data-driven mods from ContentSchemaExecutor (if not already listed above)
        for mod_id in content.loaded_mods() {
            if listed_mod_ids.contains(&mod_id) {
                continue;
            }
            let b_count = content.blocks_for_mod(&mod_id).len();
            let i_count = content.items_for_mod(&mod_id).len();

            mods.push(NativeMod {
                id: format!("rurge_mod_{}", mod_id),
                title: format!("[Converted JAR] {} [.rmod]", mod_id),
                description: format!(
                    "Data-driven converted mod (Blocks: {}, Items: {})",
                    b_count, i_count
                ),
                keys: vec![],
            });
        }
    }

    /// Populates settings buttons for the given mod ID.
    pub fn populate_settings(&self, mod_id: &str, buttons: &mut Vec<String>) {
        match mod_id {
            "rurge_core" => {
                buttons.push("Open Mods Folder (.rmod / .jar)".to_string());
                buttons.push("Open Shaderpacks Folder (.zip)".to_string());
                buttons.push("Re-scan & Convert Dropped JARs".to_string());
            }
            "rurge_shaders" => {
                let active = self.current_shader().unwrap_or_else(|| "None".to_string());
                buttons.push(format!("Active: {} (Click to Toggle)", active));
                buttons.push("Open Shaderpacks Folder".to_string());
            }
            id if id.starts_with("rurge_mod_") => {
                buttons.push("Open Mods Folder".to_string());
                buttons.push("Status: Active Native Mod".to_string());
            }
            _ => {}
        }
    }

    /// Handles a Rurge text command string without needing the graphical Host.
    pub fn handle_command_str(&self, raw_cmd: &str) -> Option<(String, Flow)> {
        let cmd = raw_cmd.trim();
        if cmd == "rurge" || cmd == "rurge help" {
            let active = self.current_shader().unwrap_or_else(|| "None".to_string());
            let count = self.engine.mod_manager().loaded_plugins().len();
            let content_blocks = self.engine.content().block_count();
            let content_items = self.engine.content().item_count();
            let msg = if content_blocks > 0 || content_items > 0 {
                format!(
                    "§6[Rurge]§r v0.2.0 (Rust) | §eActive Shader:§r {} | §aLoaded Mods:§r {} (Custom Content: {} blocks/items)",
                    active, count, content_blocks + content_items
                )
            } else {
                format!(
                    "§6[Rurge]§r v0.2.0 (Rust) | §eActive Shader:§r {} | §aLoaded Mods:§r {}",
                    active, count
                )
            };
            return Some((msg, Flow::Cancel));
        }
        if cmd == "rurge shaders" || cmd == "rurge toggle" {
            self.toggle_default_shaderpack();
            let active = self.current_shader().unwrap_or_else(|| "None".to_string());
            let msg = format!("§6[Rurge]§r Shaderpack set to: §e{}§r", active);
            return Some((msg, Flow::Cancel));
        }
        if cmd == "rurge convert" {
            let converted = crate::jar_scanner::convert_dropped_jars_in(Path::new("mods"));
            let msg = if converted.is_empty() {
                "§6[Rurge]§r No new or modified .jar mods found to convert in mods/".to_string()
            } else {
                format!("§6[Rurge]§r Converted {} JAR mod(s) to native .rmod!", converted.len())
            };
            return Some((msg, Flow::Cancel));
        }
        None
    }
}

impl Hooks for RurgeHorizonHooks {
    fn render(&self) -> Option<&dyn RenderHooks> {
        Some(self)
    }

    fn blocks(&self, event: &mut BlocksEvent<'_>) {
        for block in self.engine.content().all_blocks() {
            if !event.blocks.iter().any(|b| b.id == block.id) {
                event.blocks.push(HorizonBlock {
                    id: block.id.clone(),
                    template: "minecraft:stone".to_string(),
                    item: true,
                    ..Default::default()
                });
            }
        }
    }

    fn items(&self, event: &mut ItemsEvent<'_>) {
        for item in self.engine.content().all_items() {
            if !event.items.iter().any(|i| i.id == item.id) {
                event.items.push(HorizonItem {
                    id: item.id.clone(),
                    template: "minecraft:stick".to_string(),
                    max_stack_size: Some(item.max_stack_size.clamp(1, 99) as u8),
                    ..Default::default()
                });
            }
        }
    }

    fn shader_params(&self, _host: &dyn Host, event: &mut ShaderParamsEvent<'_>) {
        self.shader_params(event.params);
    }

    fn mods(&self, _host: &mut dyn Host, event: &mut ModsEvent<'_>) {
        self.populate_native_mods(event.mods);
    }

    fn settings(&self, _host: &mut dyn Host, event: &mut SettingsEvent<'_>) {
        self.populate_settings(event.mod_id, event.buttons);
    }

    fn settings_click(&self, _host: &mut dyn Host, event: &mut SettingsClickEvent<'_>) {
        match (event.mod_id, event.button) {
            ("rurge_core", 0) => {
                let _ = Command::new("explorer").arg("mods").spawn();
            }
            ("rurge_core", 1) => {
                let _ = Command::new("explorer").arg("shaderpacks").spawn();
            }
            ("rurge_core", 2) => {
                let converted = crate::jar_scanner::convert_dropped_jars_in(Path::new("mods"));
                log::info!("Manual re-scan converted {} JAR mod(s)", converted.len());
            }
            ("rurge_shaders", 0) => {
                self.toggle_default_shaderpack();
            }
            ("rurge_shaders", 1) => {
                let _ = Command::new("explorer").arg("shaderpacks").spawn();
            }
            (id, _) if id.starts_with("rurge_mod_") => {
                let _ = Command::new("explorer").arg("mods").spawn();
            }
            _ => {}
        }
    }

    fn key(&self, _host: &mut dyn Host, event: &mut KeyEvent<'_>) {
        if event.pressed && event.key == "key.keyboard.k" {
            self.toggle_default_shaderpack();
        }
    }

    fn commands(&self, _host: &mut dyn Host, event: &mut CommandsEvent<'_>) {
        event.commands.push("rurge");
    }

    fn command(&self, host: &mut dyn Host, event: &mut CommandEvent<'_>) -> Flow {
        if let Some((msg, flow)) = self.handle_command_str(event.command) {
            host.broadcast(Text::literal(msg));
            return flow;
        }
        Flow::Continue
    }

    fn hud(&self, host: &mut dyn Host, event: &mut HudEvent<'_>) {
        let (x, y) = event.corners.place(Corner::TopRight, 184.0, 38.0);
        // Semi-transparent background panel (dark slate)
        host.fill(event.batch, [x, y, 184.0, 38.0], 0xAA0F172A);
        // Cyan accent indicator on left border
        host.fill(event.batch, [x, y, 3.0, 38.0], 0xFF00D1FF);

        let native_count = self.engine.mod_manager().loaded_plugins().len();
        let converted_count = self.engine.content().loaded_mods().len();
        let total_count = native_count + converted_count;
        let b_count = self.engine.content().block_count();
        let i_count = self.engine.content().item_count();

        let active = self.current_shader().unwrap_or_else(|| "Vanilla (Off)".to_string());

        host.text(event.batch, "RURGE NATIVE 0.2.0", [x + 8.0, y + 4.0], 0x00D1FF, true);
        host.text(
            event.batch,
            &format!("Shaders: {}", active),
            [x + 8.0, y + 15.0],
            0xFFDD44,
            true,
        );
        let mods_label = if b_count > 0 || i_count > 0 {
            format!("Mods: {} loaded ({} items, {} blocks)", total_count, i_count, b_count)
        } else {
            format!("Mods: {} loaded (.rmod)", total_count)
        };
        host.text(
            event.batch,
            &mods_label,
            [x + 8.0, y + 26.0],
            0xCCCCCC,
            true,
        );
    }
}

impl RenderHooks for RurgeHorizonHooks {
    fn render_needs(&self, event: &mut RenderNeedsEvent<'_>) {
        event.needs.world_depth = true;

        let active_pack = Self::query_active_shaderpack();
        self.check_and_dispatch_shader_transition(active_pack);
    }

    fn world_drawn(&self, event: &mut WorldDrawnEvent<'_, '_>) {
        if !self.engine.has_world_drawn_hooks() {
            return;
        }

        let ctx = RenderPassContext {
            viewport_width: event.world.size[0],
            viewport_height: event.world.size[1],
            partial_tick: 0.0,
            camera_pos: [
                event.world.camera.x,
                event.world.camera.y,
                event.world.camera.z,
            ],
            view_matrix: event.world.view_proj.to_cols_array(),
        };

        // Create secondary command encoder to isolate GPU recording from Horizon's primary encoder.
        // If mod hook execution panics or aborts, dropping the secondary encoder without finishing
        // guarantees that Horizon's primary command encoder remains valid and unpoisoned (Anti-Goal AG1).
        let secondary_encoder = event.world.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor {
                label: Some("rurge_world_drawn_secondary_encoder"),
            },
        );

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.engine.execute_world_drawn(&ctx)
        }));

        match result {
            Ok(true) => {
                let cmd_buffer = secondary_encoder.finish();
                event.world.queue.submit([cmd_buffer]);
            }
            Ok(false) => {
                log::error!(
                    "Panic detected in mod render hook during world_drawn; dropping secondary encoder"
                );
            }
            Err(err) => {
                log::error!(
                    "Uncaught panic during Rurge world_drawn execution, dropping secondary encoder: {:?}",
                    err
                );
            }
        }
    }

    fn hand_drawn(&self, event: &mut HandDrawnEvent<'_, '_>) {
        if !self.engine.has_hand_drawn_hooks() {
            return;
        }

        let ctx = RenderPassContext {
            viewport_width: event.hand.size[0],
            viewport_height: event.hand.size[1],
            partial_tick: 0.0,
            camera_pos: [
                event.hand.camera.x,
                event.hand.camera.y,
                event.hand.camera.z,
            ],
            view_matrix: event.hand.view_proj.to_cols_array(),
        };

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.engine.execute_hand_drawn(&ctx)
        }));

        match result {
            Ok(true) => {}
            Ok(false) => {
                log::error!("Panic detected in mod render hook during hand_drawn");
            }
            Err(err) => {
                log::error!("Uncaught panic during Rurge hand_drawn execution: {:?}", err);
            }
        }
    }
}

use std::path::Path;
use std::sync::Arc;
use abi_stable::std_types::RString;
use horizon_client::mod_api::{Hooks, Host, ShaderParamsEvent};
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
        Self {
            engine,
            current_shader: RwLock::new(None),
        }
    }

    /// Safe hook point where mods can set uniform data.
    pub fn shader_params(&self, params: &mut [[f32; 4]; 8]) {
        *params = self.engine.get_shader_uniforms();
    }

    /// Returns the currently tracked active shaderpack name, if any.
    pub fn current_shader(&self) -> Option<String> {
        self.current_shader.read().clone()
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
}

impl Hooks for RurgeHorizonHooks {
    fn render(&self) -> Option<&dyn RenderHooks> {
        Some(self)
    }

    fn shader_params(&self, _host: &dyn Host, event: &mut ShaderParamsEvent<'_>) {
        self.shader_params(event.params);
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

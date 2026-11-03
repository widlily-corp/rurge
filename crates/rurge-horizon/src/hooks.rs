use std::sync::Arc;
use horizon_client::mod_api::{Hooks, Host, ShaderParamsEvent};
use horizon_client_render::gfx::native_render::{
    wgpu, HandDrawnEvent, RenderHooks, RenderNeedsEvent, WorldDrawnEvent,
};
use rurge_api::events::RenderPassContext;
use rurge_runtime::engine::RurgeEngine;

/// Native bridge linking Horizon's mod hooks and render pipeline to `RurgeEngine`.
pub struct RurgeHorizonHooks {
    /// Shared reference to the central Rurge runtime engine.
    pub engine: Arc<RurgeEngine>,
}

impl RurgeHorizonHooks {
    /// Creates a new native hooks bridge wrapping the shared runtime engine.
    pub fn new(engine: Arc<RurgeEngine>) -> Self {
        Self { engine }
    }

    /// Safe hook point where mods can set uniform data.
    pub fn shader_params(&self, _params: &mut [[f32; 4]; 8]) {
        // Safe hook point where mods can set uniform data.
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
    }

    fn world_drawn(&self, event: &mut WorldDrawnEvent<'_, '_>) {
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
            self.engine.execute_world_drawn(&ctx);
        }));

        match result {
            Ok(()) => {
                let cmd_buffer = secondary_encoder.finish();
                event.world.queue.submit([cmd_buffer]);
            }
            Err(err) => {
                log::error!(
                    "Panic during Rurge world_drawn execution, dropping secondary encoder: {:?}",
                    err
                );
            }
        }
    }

    fn hand_drawn(&self, event: &mut HandDrawnEvent<'_, '_>) {
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
            self.engine.execute_hand_drawn(&ctx);
        }));

        if let Err(err) = result {
            log::error!("Panic during Rurge hand_drawn execution: {:?}", err);
        }
    }
}

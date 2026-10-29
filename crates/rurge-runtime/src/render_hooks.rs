use std::collections::HashMap;
use parking_lot::RwLock;
use rurge_blamer::{safe_invoke, Blamer};

/// Phases in the rendering pipeline where mods can inject custom passes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RenderPassPhase {
    /// Before the primary world geometry and block render pass.
    PreRender,
    /// Post-processing shaders and full-screen effects.
    PostProcess,
    /// 2D heads-up display and GUI overlay render pass.
    HudOverlay,
}

/// Context passed to mod render hooks during frame rendering.
pub struct RenderPassContext<'a> {
    /// Current render frame sequence index.
    pub frame_index: u64,
    /// Render target viewport width in pixels.
    pub width: u32,
    /// Render target viewport height in pixels.
    pub height: u32,
    /// Optional mutable handle to render resources (e.g. WGPU CommandEncoder, TextureView, or custom state).
    pub custom_data: Option<&'a mut dyn std::any::Any>,
}

impl<'a> RenderPassContext<'a> {
    /// Creates a basic render pass context without custom state.
    pub fn new(frame_index: u64, width: u32, height: u32) -> Self {
        Self {
            frame_index,
            width,
            height,
            custom_data: None,
        }
    }

    /// Creates a render pass context with attached mutable custom state.
    pub fn with_custom_data(
        frame_index: u64,
        width: u32,
        height: u32,
        custom_data: &'a mut dyn std::any::Any,
    ) -> Self {
        Self {
            frame_index,
            width,
            height,
            custom_data: Some(custom_data),
        }
    }
}

type RenderHookFn = Box<dyn Fn(&mut RenderPassContext) + Send + Sync + 'static>;

struct RenderHookEntry {
    mod_id: String,
    hook: RenderHookFn,
}

/// Orchestrator for injecting custom mod render passes into the Horizon graphics pipeline.
#[derive(Default)]
pub struct RenderPipelineHooks {
    passes: RwLock<HashMap<RenderPassPhase, Vec<RenderHookEntry>>>,
}

impl RenderPipelineHooks {
    /// Creates a new empty `RenderPipelineHooks`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a custom render pass for a given phase attributed to `mod_id`.
    pub fn register_pass<F>(&mut self, mod_id: String, phase: RenderPassPhase, hook: F)
    where
        F: Fn(&mut RenderPassContext) + Send + Sync + 'static,
    {
        self.passes
            .write()
            .entry(phase)
            .or_default()
            .push(RenderHookEntry {
                mod_id,
                hook: Box::new(hook),
            });
    }

    /// Convenience registration for the `PreRender` phase.
    pub fn register_pre_pass<F>(&mut self, mod_id: impl Into<String>, hook: F)
    where
        F: Fn(&mut RenderPassContext) + Send + Sync + 'static,
    {
        self.register_pass(mod_id.into(), RenderPassPhase::PreRender, hook);
    }

    /// Convenience registration for the `PostProcess` phase.
    pub fn register_post_pass<F>(&mut self, mod_id: impl Into<String>, hook: F)
    where
        F: Fn(&mut RenderPassContext) + Send + Sync + 'static,
    {
        self.register_pass(mod_id.into(), RenderPassPhase::PostProcess, hook);
    }

    /// Convenience registration for the `HudOverlay` phase.
    pub fn register_hud_pass<F>(&mut self, mod_id: impl Into<String>, hook: F)
    where
        F: Fn(&mut RenderPassContext) + Send + Sync + 'static,
    {
        self.register_pass(mod_id.into(), RenderPassPhase::HudOverlay, hook);
    }

    /// Executes all hooks registered for the specified phase inside panic boundaries.
    ///
    /// If any mod panics, `safe_invoke` records the incident in `blamer` and suppresses
    /// further crashes for that frame, preventing a crash of the GPU context or render thread.
    pub fn execute_phase(
        &self,
        phase: RenderPassPhase,
        blamer: &Blamer,
        ctx: &mut RenderPassContext,
    ) {
        let lock = self.passes.read();
        if let Some(hooks) = lock.get(&phase) {
            for entry in hooks {
                let mod_id = &entry.mod_id;
                let hook = &entry.hook;
                let _ = safe_invoke(
                    mod_id,
                    "render_pass",
                    blamer,
                    std::panic::AssertUnwindSafe(|| {
                        hook(ctx);
                    }),
                );
            }
        }
    }
}

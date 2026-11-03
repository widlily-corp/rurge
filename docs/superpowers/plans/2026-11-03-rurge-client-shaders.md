# Rurge Client Runtime & Shaders Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `rurge-horizon` as the native game client runner bridging Horizon with Rurge's mod engine, enabling classic `.zip` OptiFine/Iris shaderpacks and exposing a panic-isolated Render & Shader API for `.rmod` native mods.

**Architecture:** A new workspace binary crate `crates/rurge-horizon` implements Horizon's `Hooks` and `RenderHooks` traits, bridging the game's WGPU render loop into `rurge-runtime`'s `RenderPipelineHooks`. Classic `.zip` shaderpacks placed in `shaderpacks/` are processed by Horizon's `horizon-shaders` compiler, while Rurge native mods receive ABI-stable frame metadata, custom uniform slots (`mods[8]`), and isolated secondary command encoders that prevent `wgpu::DeviceLost` during mod panics.

**Tech Stack:** Rust 2024 (rustc 1.96.0, `rust-version = "1.90"`), `wgpu 30`, `abi_stable 0.11`, `horizon-client 0.1.0`, `horizon-shaders 0.1.0`.

**Spec:** [docs/superpowers/specs/2026-11-03-rurge-client-shaders-design.md](file:///c:/Users/Widlily/Documents/projects/Rurge/docs/superpowers/specs/2026-11-03-rurge-client-shaders-design.md)

## Global Constraints

- Language & Edition: Rust edition 2024, `rust-version = "1.90"`.
- C-ABI Boundary: All cross-dylib types in `rurge-api` must implement `abi_stable::StableAbi`.
- GPU Panic Safety: Unhandled panics in mod render callbacks must never invalidate the main WGPU command encoder or trigger `wgpu::DeviceLost`.
- Memory & Performance: Zero heap allocations (`Vec::new`, string formatting) in the per-frame render loop; render passes must use secondary encoders cleanly.
- Upstream Cleanliness: Horizon crates in `../horizon` remain an unmodified upstream dependency.

## Review Focus

1. **Secondary Command Encoder Dropping on Panic**: When an `.rmod` mod panics during a draw callback, the secondary `wgpu::CommandEncoder` must be aborted without calling `.finish()`, ensuring the graphics device remains valid.
2. **Shaderpack Selection Detection**: When a player selects a classic `.zip` shaderpack in the settings menu, Rurge's `EventBus` must fire `ShaderpackLoadedEvent` on the next frame with the correct pack name.
3. **Mod Uniform Parameter Transfer**: Mod data written to custom shader uniforms must correctly map into `native_shader_params` (`mods[0..7]`) without out-of-bounds indexing.
4. **ZIP Traversal Protection for Shaderpacks**: Decompressing or reading classic `.zip` shader archives must validate paths against Zip-Slip attacks before accessing disk.
5. **Clean Shutdown with Active Shaders**: Closing the game window while running a classic shaderpack must cleanly flush pending GPU queues and trigger mod shutdown handlers without deadlock.

---

### Task 1: Workspace Alignment & Dependencies Configuration

**Files:**
- Modify: `Cargo.toml:1-40`

**Interfaces:**
- Consumes: `Cargo.toml` root
- Produces: Workspace members including `crates/rurge-horizon`, `rust-version = "1.90"`, and `[patch.crates-io]` entries for `wgpu-hal`, `naga`, and `libmimalloc-sys`.

- [ ] **Step 1: Write the failing check**

Check workspace manifests before adding `rurge-horizon`:
Run: `cargo check --workspace`
Expected: Passes without `rurge-horizon`.

- [ ] **Step 2: Update `Cargo.toml` with `rurge-horizon` and Horizon crate patches**

Update root `Cargo.toml`:
1. Bump `rust-version = "1.90"`.
2. Add `"crates/rurge-horizon"` to `workspace.members`.
3. Add `[patch.crates-io]` pointing `wgpu-hal`, `naga`, and `libmimalloc-sys` to `../horizon/third_party/...`.

- [ ] **Step 3: Create initial package manifest for `crates/rurge-horizon/Cargo.toml`**

Define `rurge-horizon`:
```toml
[package]
name = "rurge-horizon"
version = "0.1.0"
edition.workspace = true
license.workspace = true
rust-version.workspace = true
default-run = "rurge-horizon"

[dependencies]
rurge-api = { path = "../rurge-api" }
rurge-core = { path = "../rurge-core" }
rurge-blamer = { path = "../rurge-blamer" }
rurge-loader = { path = "../rurge-loader" }
rurge-runtime = { path = "../rurge-runtime" }
horizon-client = { path = "../../horizon/crates/horizon-client" }
horizon-client-render = { path = "../../horizon/crates/horizon-client-render" }
horizon-shaders = { path = "../../horizon/crates/horizon-shaders" }
wgpu = { version = "30.0.1", default-features = false }
log = "0.4"
env_logger = "0.11"
zip = { version = "9.1.3", default-features = false, features = ["deflate"] }
```

- [ ] **Step 4: Verify workspace configuration**

Run: `cargo check -p rurge-horizon`
Expected: PASS (or minimal missing `src/main.rs`).

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crates/rurge-horizon/Cargo.toml
git commit -m "chore: configure rurge-horizon package and workspace dependencies"
```

---

### Task 2: Extend `rurge-api` with Shader Events & Stable Render Types

**Files:**
- Modify: `crates/rurge-api/src/events.rs:1-120`
- Modify: `crates/rurge-api/src/lib.rs:1-80`
- Test: `crates/rurge-api/tests/shader_events_test.rs`

**Interfaces:**
- Consumes: `abi_stable` standard types (`RString`, `ROption`, `RVec`).
- Produces: `ShaderpackLoadedEvent`, `ShaderpackUnloadedEvent`, `RenderPassContext`, `ShaderUniformSlot`.

- [ ] **Step 1: Write the failing unit test in `crates/rurge-api/tests/shader_events_test.rs`**

```rust
use abi_stable::std_types::RString;
use rurge_api::events::{RenderPassContext, ShaderpackLoadedEvent, ShaderpackUnloadedEvent};

#[test]
fn test_shader_events_and_context_abi_stability() {
    let loaded = ShaderpackLoadedEvent {
        pack_name: RString::from("complementary-v5.zip"),
        is_classic: true,
    };
    assert_eq!(loaded.pack_name.as_str(), "complementary-v5.zip");
    assert!(loaded.is_classic);

    let unloaded = ShaderpackUnloadedEvent;
    let _ = unloaded;

    let ctx = RenderPassContext {
        viewport_width: 1920,
        viewport_height: 1080,
        partial_tick: 0.5,
        camera_pos: [100.0, 64.0, -200.0],
        view_matrix: [1.0; 16],
    };
    assert_eq!(ctx.viewport_width, 1920);
    assert_eq!(ctx.partial_tick, 0.5);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rurge-api --test shader_events_test`
Expected: FAIL with missing types `ShaderpackLoadedEvent`, `RenderPassContext`.

- [ ] **Step 3: Implement Shader Events and `RenderPassContext` in `crates/rurge-api/src/events.rs`**

Implement `#[derive(Clone, Debug, PartialEq, StableAbi)]` structs for `ShaderpackLoadedEvent`, `ShaderpackUnloadedEvent`, `RenderPassContext`, and re-export in `crates/rurge-api/src/lib.rs`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p rurge-api --test shader_events_test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/rurge-api/src/events.rs crates/rurge-api/src/lib.rs crates/rurge-api/tests/shader_events_test.rs
git commit -m "feat(api): add shaderpack events and ABI-stable RenderPassContext"
```

---

### Task 3: Render Pipeline Hooks & GPU Panic Boundary in `rurge-runtime`

**Files:**
- Modify: `crates/rurge-runtime/src/render.rs:1-120`
- Modify: `crates/rurge-runtime/src/engine.rs:1-150`
- Test: `crates/rurge-runtime/tests/render_pipeline_test.rs`

**Interfaces:**
- Consumes: `rurge_api::events::RenderPassContext`, `rurge_blamer::safe_invoke`.
- Produces: `RenderPipelineHooks::register_world_drawn`, `RenderPipelineHooks::execute_world_drawn`, `RenderPipelineHooks::register_hand_drawn`.

- [ ] **Step 1: Write the failing unit test in `crates/rurge-runtime/tests/render_pipeline_test.rs`**

```rust
use rurge_api::events::RenderPassContext;
use rurge_blamer::Blamer;
use rurge_runtime::render::RenderPipelineHooks;
use std::sync::{Arc, Mutex};

#[test]
fn test_render_pipeline_world_drawn_and_panic_isolation() {
    let blamer = Arc::new(Blamer::new());
    let hooks = RenderPipelineHooks::new();
    let executed = Arc::new(Mutex::new(false));
    let executed_clone = executed.clone();

    // Register a valid hook
    hooks.register_world_drawn(Box::new(move |_ctx| {
        let mut val = executed_clone.lock().unwrap();
        *val = true;
    }));

    // Register a panicking hook
    hooks.register_world_drawn(Box::new(|_ctx| {
        panic!("simulated render hook panic!");
    }));

    let ctx = RenderPassContext {
        viewport_width: 800,
        viewport_height: 600,
        partial_tick: 1.0,
        camera_pos: [0.0, 0.0, 0.0],
        view_matrix: [0.0; 16],
    };

    // Execute with blamer isolation
    hooks.execute_world_drawn(&ctx, "panicking-mod", &blamer);

    assert!(*executed.lock().unwrap(), "Valid hook must have executed");
    assert_eq!(blamer.recent_panics().len(), 1, "Panic must be trapped and blamed");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rurge-runtime --test render_pipeline_test`
Expected: FAIL with `register_world_drawn` / `execute_world_drawn` undefined.

- [ ] **Step 3: Implement `world_drawn` and `hand_drawn` hooks in `crates/rurge-runtime/src/render.rs`**

Extend `RenderPipelineHooks`:
1. Add `world_drawn_hooks: Vec<Box<dyn Fn(&RenderPassContext) + Send + Sync>>`.
2. Add `hand_drawn_hooks: Vec<Box<dyn Fn(&RenderPassContext) + Send + Sync>>`.
3. Implement `execute_world_drawn(&self, ctx: &RenderPassContext, mod_id: &str, blamer: &Blamer)` using `rurge_blamer::safe_invoke`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p rurge-runtime --test render_pipeline_test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/rurge-runtime/src/render.rs crates/rurge-runtime/src/engine.rs crates/rurge-runtime/tests/render_pipeline_test.rs
git commit -m "feat(runtime): implement world_drawn and hand_drawn hooks with blamer isolation"
```

---

### Task 4: Classic Test Shaderpack Bundle & Asset Verification

**Files:**
- Create: `assets/shaderpacks/test-classic-shader/shaders/shaders.properties`
- Create: `assets/shaderpacks/test-classic-shader/shaders/gbuffers_terrain.vsh`
- Create: `assets/shaderpacks/test-classic-shader/shaders/gbuffers_terrain.fsh`
- Create: `assets/shaderpacks/test-classic-shader/shaders/composite.vsh`
- Create: `assets/shaderpacks/test-classic-shader/shaders/composite.fsh`
- Create: `assets/shaderpacks/test-classic-shader/shaders/final.vsh`
- Create: `assets/shaderpacks/test-classic-shader/shaders/final.fsh`
- Create: `crates/rurge-horizon/src/shaderpack_bundle.rs`
- Test: `crates/rurge-horizon/tests/shaderpack_bundle_test.rs`

**Interfaces:**
- Consumes: GLSL 1.20+ format expected by `horizon-shaders::Pack`.
- Produces: `ensure_default_shaderpack_installed(target_dir: &Path) -> Result<PathBuf, String>`.

- [ ] **Step 1: Create GLSL shaderpack files**

Write standard OptiFine/Iris compatible passes:
- `shaders.properties`: `colortex0Format=RGBA8`, `sliders=VignetteStrength`.
- `gbuffers_terrain.vsh/.fsh`: basic texture map and tint color output to `gl_FragData[0]`.
- `composite.vsh/.fsh`: screen-space ambient shading and color warmth.
- `final.vsh/.fsh`: vignette and color grade output.

- [ ] **Step 2: Write failing test in `crates/rurge-horizon/tests/shaderpack_bundle_test.rs`**

```rust
use std::path::Path;

#[test]
fn test_shaderpack_bundle_installation_and_validation() {
    let tmp = tempfile::tempdir().unwrap();
    let shaderpacks_dir = tmp.path().join("shaderpacks");
    std::fs::create_dir_all(&shaderpacks_dir).unwrap();

    let pack_path = rurge_horizon::shaderpack_bundle::ensure_default_shaderpack_installed(&shaderpacks_dir)
        .expect("Must install default shaderpack");

    assert!(pack_path.exists());
    assert_eq!(pack_path.file_name().unwrap(), "test-classic-shader.zip");

    // Verify horizon-shaders can parse this pack
    let pack = horizon_shaders::Pack::open(&pack_path).expect("Pack must be recognized by horizon-shaders");
    assert!(pack.has_stage("final"));
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test -p rurge-horizon --test shaderpack_bundle_test`
Expected: FAIL with `shaderpack_bundle` missing.

- [ ] **Step 4: Implement `shaderpack_bundle.rs`**

Embed the shader files using `include_bytes!` or build a ZIP archive programmatically using `zip::ZipWriter` with Zip-Slip path normalization.

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p rurge-horizon --test shaderpack_bundle_test`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add assets/shaderpacks/ crates/rurge-horizon/src/shaderpack_bundle.rs crates/rurge-horizon/tests/shaderpack_bundle_test.rs
git commit -m "feat(horizon): package classic OptiFine/Iris test shaderpack and installer"
```

---

### Task 5: Implement `RurgeHorizonHooks` (Native Bridge to Horizon Client)

**Files:**
- Create: `crates/rurge-horizon/src/hooks.rs`
- Modify: `crates/rurge-horizon/src/lib.rs`
- Test: `crates/rurge-horizon/tests/hooks_test.rs`

**Interfaces:**
- Consumes: `horizon_client::mod_api::Hooks`, `horizon_client_render::gfx::native_render::RenderHooks`, `RurgeEngine`.
- Produces: `struct RurgeHorizonHooks`, `RurgeHorizonHooks::new(engine: Arc<RurgeEngine>)`.

- [ ] **Step 1: Write failing unit test in `crates/rurge-horizon/tests/hooks_test.rs`**

```rust
use rurge_horizon::hooks::RurgeHorizonHooks;
use rurge_runtime::engine::RurgeEngine;
use std::sync::Arc;

#[test]
fn test_rurge_horizon_hooks_creation_and_render_query() {
    let engine = Arc::new(RurgeEngine::new());
    let hooks = RurgeHorizonHooks::new(engine.clone());

    use horizon_client::mod_api::Hooks;
    assert!(hooks.render().is_some(), "RenderHooks must be provided");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rurge-horizon --test hooks_test`
Expected: FAIL with `RurgeHorizonHooks` undefined.

- [ ] **Step 3: Implement `RurgeHorizonHooks` and secondary command encoder isolation**

In `crates/rurge-horizon/src/hooks.rs`:
1. Implement `horizon_client::mod_api::Hooks for RurgeHorizonHooks`:
   - `fn render(&self) -> Option<&dyn RenderHooks> { Some(self) }`
   - `fn shader_params(&self, params: &mut [[f32; 4]; 8])`: populate `params` with mod uniforms.
2. Implement `horizon_client_render::gfx::native_render::RenderHooks for RurgeHorizonHooks`:
   - `fn render_needs(&self, event: &mut RenderNeedsEvent<'_>)`: set `event.needs.world_depth = true`.
   - `fn world_drawn(&self, event: &mut WorldDrawnEvent<'_, '_>)`:
     - Construct `RenderPassContext`.
     - Create an isolated secondary `wgpu::CommandEncoder`.
     - Execute `rurge-runtime`'s `world_drawn` hooks within `safe_invoke`.
     - On success: submit secondary command buffer to `event.world.queue`.
     - On panic: drop encoder, suppress mod, and keep primary `event.world.encoder` untouched.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p rurge-horizon --test hooks_test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/rurge-horizon/src/hooks.rs crates/rurge-horizon/src/lib.rs crates/rurge-horizon/tests/hooks_test.rs
git commit -m "feat(horizon): implement RurgeHorizonHooks bridging RenderHooks with secondary encoder isolation"
```

---

### Task 6: Client Main Entrypoint & CLI Launch Harness

**Files:**
- Create: `crates/rurge-horizon/src/main.rs`
- Modify: `crates/cargo-rurge/src/main.rs:1-150`
- Test: `tests/client_runner_test.rs`

**Interfaces:**
- Consumes: `RurgeHorizonHooks`, `horizon_client::entry::main_with`.
- Produces: `rurge-horizon` binary, `cargo rurge run` command.

- [ ] **Step 1: Write failing test in `tests/client_runner_test.rs`**

```rust
#[test]
fn test_rurge_horizon_bootstrap_smoke() {
    let engine = rurge_runtime::engine::RurgeEngine::new();
    let hooks = rurge_horizon::hooks::RurgeHorizonHooks::new(std::sync::Arc::new(engine));
    // Verify hooks type signature matches horizon main_with expectation
    let mods: horizon_client::mod_api::Mods = std::sync::Arc::new([Box::new(hooks)]);
    assert_eq!(mods.len(), 1);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test client_runner_test`
Expected: FAIL.

- [ ] **Step 3: Implement `crates/rurge-horizon/src/main.rs`**

1. Initialize logging with `env_logger`.
2. Discover `mods/` and load `.rmod` dynamic libraries.
3. Call `ensure_default_shaderpack_installed("shaderpacks")`.
4. Construct `RurgeHorizonHooks`.
5. Call `horizon_client::entry::main_with(Arc::new([Box::new(hooks)]))`.

- [ ] **Step 4: Add `run` subcommand to `cargo-rurge`**

In `crates/cargo-rurge/src/main.rs`, add `cargo rurge run` which invokes `cargo run -p rurge-horizon`.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --test client_runner_test`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/rurge-horizon/src/main.rs crates/cargo-rurge/src/main.rs tests/client_runner_test.rs
git commit -m "feat(client): implement rurge-horizon main entrypoint and cargo rurge run"
```

---

### Task 7: Example Shader-Aware Mod (`examples/shader-hud-mod`)

**Files:**
- Create: `examples/shader-hud-mod/Cargo.toml`
- Create: `examples/shader-hud-mod/rurge.toml`
- Create: `examples/shader-hud-mod/src/lib.rs`
- Test: `tests/shader_mod_integration_test.rs`

**Interfaces:**
- Consumes: `rurge_api::RurgePlugin`, `ShaderpackLoadedEvent`, `RenderPassContext`.
- Produces: Compiled dynamic library demonstrating HUD overlay drawing and shader event handling.

- [ ] **Step 1: Write failing integration test in `tests/shader_mod_integration_test.rs`**

```rust
#[test]
fn test_shader_aware_mod_event_dispatch() {
    let engine = rurge_runtime::engine::RurgeEngine::new();
    let event_bus = engine.event_bus();

    let received = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let received_clone = received.clone();

    event_bus.register(0, move |_event: &rurge_api::events::ShaderpackLoadedEvent| {
        received_clone.store(true, std::sync::atomic::Ordering::Relaxed);
    });

    event_bus.dispatch(&rurge_api::events::ShaderpackLoadedEvent {
        pack_name: abi_stable::std_types::RString::from("test-classic-shader.zip"),
        is_classic: true,
    });

    assert!(received.load(std::sync::atomic::Ordering::Relaxed), "ShaderpackLoadedEvent must be delivered to registered mod handlers");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test shader_mod_integration_test`
Expected: FAIL.

- [ ] **Step 3: Implement `examples/shader-hud-mod`**

Implement `ShaderHudMod` implementing `RurgePlugin`:
- Subscribes to `ShaderpackLoadedEvent` to display active shaderpack name on screen.
- Registers a `world_drawn` hook to display shader status and FPS metrics.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test shader_mod_integration_test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add examples/shader-hud-mod/ tests/shader_mod_integration_test.rs
git commit -m "example: add shader-hud-mod demonstrating shaderpack events and render hooks"
```

---

### Task 8: End-to-End Verification, Clippy Audit & Smoke Test

**Files:**
- Modify: `README.md`
- Test: Workspace test suite

**Interfaces:**
- Consumes: Full codebase.
- Produces: 100% passing tests, 0 warnings in Clippy, documented instructions to run the game with shaders.

- [ ] **Step 1: Run complete workspace test suite**

Run: `cargo test --workspace`
Expected: All tests pass with 0 errors.

- [ ] **Step 2: Run clippy across all workspace targets**

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: 0 warnings.

- [ ] **Step 3: Update `README.md` with Client & Shaderpack Usage Guide**

Document:
1. How to run Rurge Client: `cargo run -p rurge-horizon` (or `cargo rurge run`).
2. How to enable shaders: *Options > Horizon Settings > Shader Packs > Select test-classic-shader.zip*.
3. How to write a shader-aware `.rmod` plugin.

- [ ] **Step 4: Commit**

```bash
git add README.md
git commit -m "docs: document rurge-horizon client runner and shaderpack configuration"
```

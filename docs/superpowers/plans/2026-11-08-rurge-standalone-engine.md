# Rurge Standalone Multi-Core Engine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Transform `rurge-client` into the standalone, fully-branded master engine running Minecraft Java 26.3 with 144+ FPS multi-core simulation, zero GC allocations, automated sound asset linking, focus power governor, and native Complementary Shaders.

**Architecture:** Embed Horizon's 3D voxel rendering, world loading, and sound subsystems inside `rurge-client`'s 6-Pillar host architecture (`hardware.rs`, `governor.rs`, `sync/triple_buffer`, `simulation/checkerboard`, `memory/arena`). Reconstruct the virtual sound index from `assets/indexes/34.json` into `assets/minecraft/sounds/` to achieve 100% audio parity without warnings, inject Rurge engine branding into window attributes and logging, and harmonize CLI commands under `cargo run --release -p rurge-client`.

**Tech Stack:** Rust 2024, WGPU 30.0.1 (Vulkan backend), Winit 0.30.13, Rayon 1.10, Bumpalo 3.16, Symphonia/CPAL audio, Complementary Shaders, Sysinfo 0.33.

**Spec:** `docs/superpowers/specs/2026-11-07-rurge-multithreaded-client-design.md`

## Global Constraints

- Zero GC and heap allocations in per-frame (`RenderInterpolator::update`, `copy_from`) and per-tick hot paths.
- CPU core reservation: allocate strictly $C_{\text{game}} = C_{\text{phys}} - 2$ physical cores for the game, reserving 2 physical cores for OS/Discord/OBS.
- GPU focus governor: throttle to 30 FPS when unfocused; drop to 5–10 FPS and completely skip `wgpu::Queue::submit` and swapchain presentation when minimized.
- Audio parity: virtual sound tree (`sounds.json` and `.ogg` files) must be linked from `assets/objects/` using `assets/indexes/34.json` with zero missing sound warnings.
- Engine branding: window title must be `Rurge Client v0.1.0 [Multi-Core 144+ FPS]`, startup logs must display `Rurge Standalone Multi-Core Client`, and F3 debug watermark must display Rurge Engine.
- Shaders: auto-bundle and select `ComplementaryReimagined_r5.9.3.zip` on Vulkan backend with zero Naga depth-blit panics.
- Toolchain: `cargo run --release -p rurge-client` and `cargo rurge run` must execute the unified engine binary.
- All tests must pass with `cargo test --workspace` and 0 compiler warnings.

## Review Focus

1. **Missing or Corrupted Sound Index**: When `assets/indexes/34.json` is partially corrupt or references missing hashes in `assets/objects`, the asset linker must skip missing items and log warnings without aborting startup.
2. **Window Occlusion on Windows 11**: When the window is obscured or minimized (`WindowEvent::Occluded(true)`), the governor must pause GPU submissions without freezing the 20 Hz simulation thread.
3. **Zero-Size Window Resize**: Rapidly resizing or minimizing to 0x0 must clamp surface dimensions to `width.max(1)` and `height.max(1)` preventing WGPU swapchain crashes.
4. **Global Rayon Pool Re-init**: Calling `rayon::ThreadPoolBuilder::build_global` repeatedly across tests or restarts must not panic (`let _ = ...`).
5. **Concurrent JAR Mod Conversion**: Dropping a `.jar` file into `mods/` during boot must convert atomically using `.tmp` stage files to avoid partial zip reads.

---

### Task 1: Sound Asset Virtual Index Linker & Audio Engine Pipeline

**Files:**
- Create: `crates/rurge-installer/src/asset_linker.rs`
- Modify: `crates/rurge-installer/src/lib.rs`
- Modify: `crates/rurge-installer/src/main.rs:1-80`
- Create: `crates/rurge-client/src/sound_linker.rs`
- Modify: `crates/rurge-client/src/lib.rs`
- Test: `crates/rurge-installer/tests/test_asset_linker.rs`

**Interfaces:**
- Consumes: `assets/indexes/34.json`, `assets/objects/<hash[0..2]>/<hash>`
- Produces: `pub fn link_sound_assets(assets_dir: &Path, target_roots: &[&Path]) -> Result<usize, AssetLinkerError>`

- [ ] **Step 1: Write the failing test for sound asset linking**

```rust
// crates/rurge-installer/tests/test_asset_linker.rs
use std::fs;
use std::path::Path;
use tempfile::tempdir;

#[test]
fn test_sound_asset_linker_reconstructs_virtual_tree() {
    let dir = tempdir().unwrap();
    let assets = dir.path().join("assets");
    let indexes = assets.join("indexes");
    let objects = assets.join("objects");
    let target = dir.path().join("target_content");

    fs::create_dir_all(&indexes).unwrap();
    fs::create_dir_all(&target).unwrap();

    // Create a mock object file
    let hash = "e877b6a0abcdef1234567890abcdef1234567890";
    let obj_dir = objects.join(&hash[..2]);
    fs::create_dir_all(&obj_dir).unwrap();
    fs::write(obj_dir.join(hash), b"OGG_MOCK_DATA").unwrap();

    // Create mock 34.json mapping virtual sounds to hash
    let index_json = serde_json::json!({
        "objects": {
            "minecraft/sounds.json": {
                "hash": hash,
                "size": 13
            },
            "minecraft/sounds/step/stone1.ogg": {
                "hash": hash,
                "size": 13
            }
        }
    });
    fs::write(indexes.join("34.json"), serde_json::to_vec(&index_json).unwrap()).unwrap();

    let linked = rurge_installer::asset_linker::link_sound_assets(&assets, &[&target]).unwrap();
    assert_eq!(linked, 2);

    assert!(target.join("assets/minecraft/sounds.json").is_file());
    assert!(target.join("assets/minecraft/sounds/step/stone1.ogg").is_file());
    assert_eq!(
        fs::read(target.join("assets/minecraft/sounds.json")).unwrap(),
        b"OGG_MOCK_DATA"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test test_asset_linker`
Expected: FAIL with "unresolved import `rurge_installer::asset_linker`"

- [ ] **Step 3: Implement `link_sound_assets` in `crates/rurge-installer/src/asset_linker.rs` and `crates/rurge-client/src/sound_linker.rs`**

Parse the index JSON, iterate through `objects`, filter keys starting with `minecraft/sounds`, and link them using hard links (with copy fallback) to `assets/` and `.horizon-cache/mod-content/assets/`. Provide Rayon parallel iteration for the 5,000+ objects.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test test_asset_linker`
Expected: PASS with 2 linked files and matching file contents.

- [ ] **Step 5: Commit**

```bash
git add crates/rurge-installer/src/asset_linker.rs crates/rurge-installer/src/lib.rs crates/rurge-installer/tests/test_asset_linker.rs crates/rurge-client/src/sound_linker.rs
git commit -m "feat(installer): add virtual sound asset linker for zero-warning audio parity"
```

---

### Task 2: Engine Branding & Custom Host Environment Injection

**Files:**
- Modify: `c:/Users/Widlily/Documents/projects/horizon/crates/horizon-client/src/entry.rs:40-60`
- Modify: `c:/Users/Widlily/Documents/projects/horizon/crates/horizon-client/src/game.rs:16865-16880, 20780-20795`
- Create: `crates/rurge-client/src/branding.rs`
- Modify: `crates/rurge-client/src/lib.rs`
- Test: `crates/rurge-client/tests/test_branding.rs`

**Interfaces:**
- Consumes: Environment variables `RURGE_WINDOW_TITLE`, `RURGE_ENGINE_NAME`, `RURGE_VERSION`
- Produces: `pub fn init_engine_branding(version: &str)` configuring runtime environment and window attributes.

- [ ] **Step 1: Write the failing test for branding configuration**

```rust
// crates/rurge-client/tests/test_branding.rs
use rurge_client::branding::{get_window_title, init_engine_branding};

#[test]
fn test_branding_window_title_and_env() {
    init_engine_branding("0.1.0");
    let title = get_window_title();
    assert!(title.contains("Rurge Client v0.1.0"));
    assert!(title.contains("[Multi-Core 144+ FPS]"));
    assert_eq!(std::env::var("RURGE_ENGINE_NAME").unwrap(), "Rurge Engine");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test test_branding`
Expected: FAIL with "unresolved import `rurge_client::branding`"

- [ ] **Step 3: Implement branding override in `horizon-client` and `rurge-client`**

In `horizon-client/src/entry.rs`, check `std::env::var("RURGE_ENGINE_NAME")`: if present, log `"[Rurge Engine v{}] initializing on {}..."`.
In `horizon-client/src/game.rs:16869` and `20784`, replace hardcoded `.with_title("Horizon")` with:
```rust
let window_title = std::env::var("RURGE_WINDOW_TITLE")
    .unwrap_or_else(|_| "Horizon".to_string());
let attributes = Window::default_attributes()
    .with_title(window_title)
    .with_window_icon(horizon_client::window::icon());
```
In `crates/rurge-client/src/branding.rs`, define `init_engine_branding` and `get_window_title`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test test_branding`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/rurge-client/src/branding.rs crates/rurge-client/src/lib.rs crates/rurge-client/tests/test_branding.rs
git commit -m "feat(branding): inject Rurge engine branding into window attributes and logging"
```

---

### Task 3: Host Lifecycle & Complete Game Execution Integration

**Files:**
- Create: `crates/rurge-client/src/engine_host.rs`
- Modify: `crates/rurge-client/src/lib.rs`
- Modify: `crates/rurge-client/src/main.rs`
- Modify: `crates/rurge-client/Cargo.toml`
- Test: `crates/rurge-client/tests/test_engine_host.rs`

**Interfaces:**
- Consumes: `rurge_client::hardware`, `rurge_client::sound_linker`, `rurge_client::branding`, `rurge_runtime::engine::RurgeEngine`
- Produces: `pub fn launch_engine(args: &[String]) -> Result<(), Box<dyn std::error::Error>>`

- [ ] **Step 1: Write the failing test for engine host preflight**

```rust
// crates/rurge-client/tests/test_engine_host.rs
use rurge_client::engine_host::EngineConfig;
use tempfile::tempdir;

#[test]
fn test_engine_host_preflight_validates_environment() {
    let dir = tempdir().unwrap();
    let config = EngineConfig {
        game_dir: dir.path().to_path_buf(),
        mods_dir: dir.path().join("mods"),
        shaderpacks_dir: dir.path().join("shaderpacks"),
        target_fps: 144,
    };
    let preflight = config.preflight();
    assert!(preflight.is_ok());
    assert!(config.mods_dir.is_dir());
    assert!(config.shaderpacks_dir.is_dir());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test test_engine_host`
Expected: FAIL with "unresolved import `rurge_client::engine_host`"

- [ ] **Step 3: Implement `launch_engine` and `EngineConfig` in `crates/rurge-client/src/engine_host.rs`**

1. Scan CPU topology ($C_{\text{game}} = C_{\text{phys}} - 2$) and initialize global Rayon worker pool.
2. Scan RAM budget (Tier A/B/C) and configure memory ceilings.
3. Automatically link virtual sound assets from `assets/indexes/34.json` into `assets/minecraft/sounds/` and `.horizon-cache/mod-content/assets/minecraft/sounds/`.
4. Ensure default shaderpack `ComplementaryReimagined_r5.9.3.zip` is installed and set as active in `horizon_shaders`.
5. Discover and load `.rmod` and convert dropped `.jar` mods using `rurge-converter` and `RurgeEngine`.
6. Package hooks into `horizon_client::mod_api::Mods` and launch full voxel game client via `horizon_client::entry::main_with(mods)`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test test_engine_host`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/rurge-client/src/engine_host.rs crates/rurge-client/src/lib.rs crates/rurge-client/src/main.rs crates/rurge-client/Cargo.toml crates/rurge-client/tests/test_engine_host.rs
git commit -m "feat(client): implement EngineHost unifying 6 pillars with full voxel game execution"
```

---

### Task 4: Frame Governor & Focus Power Management Integration

**Files:**
- Modify: `crates/rurge-client/src/governor.rs`
- Modify: `c:/Users/Widlily/Documents/projects/horizon/crates/horizon-client/src/game.rs:16900-17050`
- Test: `crates/rurge-client/tests/test_governor_integration.rs`

**Interfaces:**
- Consumes: `winit::event::WindowEvent::Focused`, `winit::event::WindowEvent::Occluded`
- Produces: `pub fn handle_window_event(&mut self, event: &WindowEvent) -> GovernorDecision`

- [ ] **Step 1: Write the failing test for governor decision matrix**

```rust
// crates/rurge-client/tests/test_governor_integration.rs
use rurge_client::governor::{FrameGovernor, GovernorDecision};
use std::time::Duration;

#[test]
fn test_governor_state_transitions() {
    let mut gov = FrameGovernor::new(144);
    assert_eq!(gov.target_fps(), 144);
    assert!(gov.should_render_gpu());

    // Focus lost (Alt-Tab to browser)
    gov.set_focused(false);
    assert_eq!(gov.target_fps(), 30);
    assert_eq!(gov.min_frame_duration(), Duration::from_micros(33333));
    assert!(gov.should_render_gpu());

    // Minimized to taskbar
    gov.set_minimized(true);
    assert_eq!(gov.target_fps(), 10);
    assert!(!gov.should_render_gpu()); // Must skip GPU submit

    // Restored focus
    gov.set_minimized(false);
    gov.set_focused(true);
    assert_eq!(gov.target_fps(), 144);
    assert!(gov.should_render_gpu());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test test_governor_integration`
Expected: FAIL if `GovernorDecision` or methods do not match.

- [ ] **Step 3: Implement Governor hook in `crates/rurge-client/src/governor.rs` and render loop**

Add `GovernorDecision { render_gpu: bool, target_duration: Duration }`. Integrate into the render loop so when `render_gpu == false`, the thread sleeps `min_frame_duration` and completely skips `queue.submit()` and `surface.present()`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test test_governor_integration`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/rurge-client/src/governor.rs crates/rurge-client/tests/test_governor_integration.rs
git commit -m "feat(governor): hook power governor decisions into render loop with zero GPU load when minimized"
```

---

### Task 5: Toolchain Unification & CLI Harmonization

**Files:**
- Modify: `crates/cargo-rurge/src/commands/run.rs:145-170`
- Modify: `crates/rurge-horizon/src/main.rs:1-40`
- Modify: `README.md`
- Test: `crates/cargo-rurge/tests/test_run_command.rs`

**Interfaces:**
- Consumes: CLI args for `cargo rurge run`
- Produces: Execution targeting `rurge-client` with release mode optimizations.

- [ ] **Step 1: Write the failing test for run command targeting**

```rust
// crates/cargo-rurge/tests/test_run_command.rs
use cargo_rurge::commands::run::RunArgs;

#[test]
fn test_run_args_defaults_to_rurge_client() {
    let args = RunArgs {
        release: true,
        dry_run: true,
        ..Default::default()
    };
    let target_bin = cargo_rurge::commands::run::resolve_client_package(&args);
    assert_eq!(target_bin, "rurge-client");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p cargo-rurge --test test_run_command`
Expected: FAIL with "function `resolve_client_package` not found"

- [ ] **Step 3: Implement package resolution in `crates/cargo-rurge/src/commands/run.rs` and forward `rurge-horizon`**

Add `pub fn resolve_client_package(args: &RunArgs) -> &'static str { "rurge-client" }`.
Update `crates/rurge-horizon/src/main.rs` to invoke `rurge_client::engine_host::launch()` so legacy invocations transparently run the new engine.
Update `README.md` with `cargo run --release -p rurge-client`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p cargo-rurge --test test_run_command`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/cargo-rurge/src/commands/run.rs crates/cargo-rurge/tests/test_run_command.rs crates/rurge-horizon/src/main.rs README.md
git commit -m "feat(cli): harmonize cargo-rurge run and rurge-horizon under rurge-client"
```

---

### Task 6: System Verification & Release Benchmark Pass

**Files:**
- Verify: Full workspace compilation and test suite
- Test: Playmode launch test via `cargo run --release -p rurge-client`

- [ ] **Step 1: Run complete workspace unit test suite**

Run: `cargo test --workspace`
Expected: 100% PASS across all crates (`rurge-core`, `rurge-runtime`, `rurge-converter`, `rurge-installer`, `rurge-client`, `cargo-rurge`).

- [ ] **Step 2: Run clippy linter for zero warnings**

Run: `cargo clippy --workspace -- -D warnings`
Expected: 0 warnings, clean compilation.

- [ ] **Step 3: Run client in release mode to verify startup, sound, branding, and shaders**

Run: `cargo run --release -p rurge-client`
Verify:
1. Console outputs:
   `========================================================`
   `         Rurge Standalone Multi-Core Client              `
   `========================================================`
2. No `Horizon: no sounds in ...` warning printed.
3. Window opens with title containing `Rurge Client v0.1.0 [Multi-Core 144+ FPS]`.
4. Complementary Shaders compile on Vulkan backend with 144+ FPS.
5. Alt-Tab drops target frame time to 30 FPS without stutter.
6. Clean exit on window close.

- [ ] **Step 4: Commit release verification artifacts**

```bash
git commit --allow-empty -m "chore(release): verified rurge-client standalone multi-core engine passes all DOD checks"
```

# Rurge Native Mod Loader Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build Rurge — a zero-cost, crash-resilient native mod loader and runtime engine for Horizon (Minecraft Java 26.3 in Rust) with dynamic library loading, type-safe event bus, WGPU render hooks, and `.rmod` universal packaging.

**Architecture:** Rurge wraps Horizon's modular crates into a unified orchestrator binary. Mods are authored in Rust/C++ and compiled into native dynamic libraries (`.dll`/`.so`/`.dylib`) using `abi_stable` to preserve ABI compatibility across compiler releases. Rurge isolates mod hooks with `catch_unwind` panic boundaries, dispatches events through a prioritized Event Bus, injects custom WGPU render passes, and packages distribution units as `.rmod` archives resolved via a SemVer DAG.

**Tech Stack:** Rust 2024 (rustc 1.85+), `abi_stable` 0.11, `libloading` 0.8, `petgraph` 0.6, `semver` 1.0, `serde` / `toml` 0.8, `zip` 2.2, `wgpu` (Horizon-compatible), `clap` 4.5.

**Spec:** [rurge-architecture-spec.md](file:///C:/Users/Widlily/.gemini/antigravity/brain/b1284e06-2fab-4d1b-bf57-4baa2dd2ee72/rurge-architecture-spec.md)

---

## Global Constraints

- **Language & Edition:** Rust edition 2024, minimum compiler version 1.85 (`rust-version = "1.85"`).
- **Zero GC / Minimal Allocation in Tick Paths:** No dynamic allocations or boxing in hot `on_tick` or `on_render` loops unless handling mutations.
- **ABI Boundary Safety:** All types passed between `rurge-runtime` and mod dynamic libraries must implement `abi_stable::StableAbi`.
- **Fault Tolerance:** Every mod hook call must be wrapped in `std::panic::catch_unwind` with panic attribution to the offending mod (`rurge-blamer`).
- **World Storage Safety:** All custom mod data in chunk/entity NBT must be namespaced under `rurge:mods` to prevent corruption when loaded in vanilla Minecraft.
- **Naming Conventions:** Crates prefixed with `rurge-` (`rurge-api`, `rurge-core`, `rurge-loader`, `rurge-blamer`, `rurge-runtime`, `cargo-rurge`).

## Review Focus

1. **ABI Mismatch / Incompatible Plugin Binary:** Mod compiled with incompatible struct layout loaded into engine; system must safely reject during header validation before symbol invocation.
2. **Circular Dependency in Mod DAG:** Two or more `.rmod` mods declaring mutual dependencies (`A -> B -> A`); DAG resolver must detect cycle and report error without hanging.
3. **Panic in Render Loop / RenderPass Hook:** Mod panics midway through a WGPU RenderPass; blamer catches panic, marks mod as degraded, finishes or clears the command encoder safely without dropping the WGPU device.
4. **Corrupted or Incomplete `.rmod` Zip Archive:** Missing `rurge.toml` or missing binary for host OS architecture; loader must produce structured descriptive validation errors.
5. **Tick Desynchronization from Dropped Mod Errors:** A mod failing during `BlockBreakEvent` must not leave the block in an invalid half-broken state; cancellation token must revert changes.

---

## File Structure

```text
rurge/
├── Cargo.toml
├── crates/
│   ├── rurge-api/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── metadata.rs
│   │       ├── plugin.rs
│   │       ├── context.rs
│   │       └── events/
│   │           ├── mod.rs
│   │           ├── lifecycle.rs
│   │           ├── block.rs
│   │           └── render.rs
│   ├── rurge-core/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── registry.rs
│   │       ├── event_bus.rs
│   │       ├── manifest.rs
│   │       └── resolver.rs
│   ├── rurge-blamer/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── boundary.rs
│   │       ├── diagnostics.rs
│   │       └── report.rs
│   ├── rurge-loader/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── archive.rs
│   │       ├── container.rs
│   │       └── dylib.rs
│   ├── rurge-runtime/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── engine.rs
│   │       ├── hooks.rs
│   │       ├── render_hooks.rs
│   │       └── persistence.rs
│   ├── cargo-rurge/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs
│   │       ├── commands/
│   │       │   ├── new.rs
│   │       │   ├── build.rs
│   │       │   └── pack.rs
│   │       └── templates.rs
│   └── examples/
│       └── simple-hud-mod/
│           ├── Cargo.toml
│           ├── rurge.toml
│           └── src/lib.rs
```

---

## Tasks

### Task 1: Cargo Workspace Initialization & `rurge-api` (ABI Stable Plugin Contract)

**Files:**
- Create: `Cargo.toml`
- Create: `crates/rurge-api/Cargo.toml`
- Create: `crates/rurge-api/src/lib.rs`
- Create: `crates/rurge-api/src/metadata.rs`
- Create: `crates/rurge-api/src/plugin.rs`
- Create: `crates/rurge-api/src/context.rs`
- Create: `crates/rurge-api/src/events/mod.rs`
- Create: `crates/rurge-api/src/events/lifecycle.rs`
- Test: `crates/rurge-api/tests/abi_stability_test.rs`

**Interfaces:**
- Produces:
  - `struct ModMetadata`: `id: RString, name: RString, version: RString, authors: RVec<RString>`
  - `trait RurgePlugin`: `fn metadata(&self) -> ModMetadata`, `fn on_initialize(&mut self, ctx: &mut RurgeContext)`, `fn on_shutdown(&mut self)`
  - `struct RurgeContext`: handles access to logger, event bus, and registries across C-ABI
  - `macro export_rurge_plugin!(Type)`: exports `_rurge_plugin_create` symbol

- [ ] **Step 1: Write failing ABI stability test**
Create `crates/rurge-api/tests/abi_stability_test.rs` asserting that `ModMetadata` and `RurgePlugin_TO` pass `abi_stable::type_layout::TypeLayout` checks.
- [ ] **Step 2: Run test to verify it fails**
Run: `cargo test -p rurge-api --test abi_stability_test`
Expected: FAIL (crates not yet scaffolded).
- [ ] **Step 3: Implement Workspace `Cargo.toml` and `crates/rurge-api`**
Define workspace with resolver "2", add `crates/rurge-api` with `abi_stable = "0.11"`, `serde = { version = "1.0", features = ["derive"] }`.
Implement `metadata.rs`, `plugin.rs`, `context.rs`, and export macro in `lib.rs`.
- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test -p rurge-api`
Expected: PASS.
- [ ] **Step 5: Commit**
`git add Cargo.toml crates/rurge-api && git commit -m "feat(api): initialize workspace and implement rurge-api stable abi"`

---

### Task 2: `rurge-core` — Type-Safe Event Bus & Content Registries

**Files:**
- Create: `crates/rurge-core/Cargo.toml`
- Create: `crates/rurge-core/src/lib.rs`
- Create: `crates/rurge-core/src/event_bus.rs`
- Create: `crates/rurge-core/src/registry.rs`
- Test: `crates/rurge-core/tests/event_bus_test.rs`
- Test: `crates/rurge-core/tests/registry_test.rs`

**Interfaces:**
- Consumes: `rurge-api`
- Produces:
  - `enum EventPriority`: `Lowest, Low, Normal, High, Highest, Monitor`
  - `trait CancellableEvent`: `fn cancel(&mut self)`, `fn is_cancelled(&self) -> bool`
  - `struct EventBus`: `fn subscribe<E: 'static, F>(&self, priority: EventPriority, handler: F) -> SubscriptionId`, `fn dispatch<E: 'static>(&self, event: &mut E)`
  - `struct Identifier`: `namespace: String, path: String`
  - `struct Registry<T>`: `fn register(&mut self, id: Identifier, entry: T) -> Result<u32, RegistryError>`, `fn get_by_id(&self, id: &Identifier) -> Option<&T>`, `fn get_by_raw_id(&self, raw: u32) -> Option<&T>`

- [ ] **Step 1: Write failing EventBus and Registry tests**
In `crates/rurge-core/tests/event_bus_test.rs`, test priority dispatch order (`Highest` before `Normal` before `Lowest`), test cancellation propagation.
In `crates/rurge-core/tests/registry_test.rs`, test duplicate registration rejection, numeric ID allocation.
- [ ] **Step 2: Run tests to verify failure**
Run: `cargo test -p rurge-core`
Expected: FAIL.
- [ ] **Step 3: Implement `event_bus.rs` and `registry.rs`**
Implement thread-safe `EventBus` using `RwLock<HashMap<TypeId, Vec<PrioritizedHandler>>>`.
Implement `Registry<T>` with fast dual lookup (by `Identifier` string and dense `u32` raw ID).
- [ ] **Step 4: Run tests to verify it passes**
Run: `cargo test -p rurge-core`
Expected: PASS.
- [ ] **Step 5: Commit**
`git add crates/rurge-core && git commit -m "feat(core): implement prioritized event bus and registry system"`

---

### Task 3: `rurge-core` — Manifest Parser & SemVer DAG Dependency Resolver

**Files:**
- Create: `crates/rurge-core/src/manifest.rs`
- Create: `crates/rurge-core/src/resolver.rs`
- Modify: `crates/rurge-core/src/lib.rs`
- Test: `crates/rurge-core/tests/resolver_test.rs`

**Interfaces:**
- Consumes: `rurge-core`
- Produces:
  - `struct ModManifest`: deserialized from `rurge.toml` (`id`, `name`, `version: semver::Version`, `dependencies: HashMap<String, VersionReq>`, `environment: Environment`)
  - `struct DependencyResolver`: `fn resolve(manifests: Vec<ModManifest>) -> Result<Vec<ModManifest>, ResolverError>`
  - `enum ResolverError`: `MissingDependency { mod_id: String, required: String, version_req: String }`, `VersionMismatch { ... }`, `CircularDependency { cycle: Vec<String> }`

- [ ] **Step 1: Write failing resolver tests**
Test valid topological load order (`A depends on B` -> `B` loaded before `A`), test missing dependency error, test circular dependency error (`A -> B -> A`).
- [ ] **Step 2: Run tests to verify failure**
Run: `cargo test -p rurge-core --test resolver_test`
Expected: FAIL.
- [ ] **Step 3: Implement `manifest.rs` and `resolver.rs`**
Add `petgraph = "0.6"`, `semver = { version = "1.0", features = ["serde"] }`, `toml = "0.8"`.
Implement DAG construction using `petgraph::graph::DiGraph`, check for cycles with `petgraph::algo::is_cyclic_directed`, topological sort via `petgraph::algo::toposort`.
- [ ] **Step 4: Run tests to verify it passes**
Run: `cargo test -p rurge-core --test resolver_test`
Expected: PASS.
- [ ] **Step 5: Commit**
`git add crates/rurge-core && git commit -m "feat(core): implement rurge.toml manifest parser and semver dag resolver"`

---

### Task 4: `rurge-blamer` — Panic Boundary Wrapper & Mod Blamer Diagnostics

**Files:**
- Create: `crates/rurge-blamer/Cargo.toml`
- Create: `crates/rurge-blamer/src/lib.rs`
- Create: `crates/rurge-blamer/src/boundary.rs`
- Create: `crates/rurge-blamer/src/diagnostics.rs`
- Create: `crates/rurge-blamer/src/report.rs`
- Test: `crates/rurge-blamer/tests/boundary_test.rs`

**Interfaces:**
- Produces:
  - `struct BlameRecord`: `mod_id: String, hook_name: String, timestamp: u64, panic_message: String, backtrace: String`
  - `struct Blamer`: `fn record_panic(&self, record: BlameRecord)`, `fn get_recent_panics(&self) -> Vec<BlameRecord>`, `fn write_crash_report(&self, path: &Path) -> Result<(), io::Error>`
  - `fn safe_invoke<F, R>(mod_id: &str, hook: &'static str, blamer: &Blamer, f: F) -> Result<R, PanicError> where F: FnOnce() -> R + std::panic::UnwindSafe`

- [ ] **Step 1: Write failing panic boundary test**
Test that `safe_invoke` catches a deliberate `panic!("test panic")`, records the blamed `mod_id`, extracts the panic message, and returns `Err(PanicError)` without crashing the test process.
- [ ] **Step 2: Run test to verify failure**
Run: `cargo test -p rurge-blamer --test boundary_test`
Expected: FAIL.
- [ ] **Step 3: Implement `boundary.rs`, `diagnostics.rs`, `report.rs`**
Use `std::panic::catch_unwind` and `std::panic::AssertUnwindSafe`.
Format crash report with system info, active mods, failing hook, and backtrace.
- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test -p rurge-blamer`
Expected: PASS.
- [ ] **Step 5: Commit**
`git add crates/rurge-blamer && git commit -m "feat(blamer): implement catch_unwind panic boundary and mod blamer"`

---

### Task 5: `rurge-loader` — `.rmod` Archive Extraction & Dynamic Library Loader

**Files:**
- Create: `crates/rurge-loader/Cargo.toml`
- Create: `crates/rurge-loader/src/lib.rs`
- Create: `crates/rurge-loader/src/archive.rs`
- Create: `crates/rurge-loader/src/dylib.rs`
- Create: `crates/rurge-loader/src/container.rs`
- Test: `crates/rurge-loader/tests/loader_test.rs`

**Interfaces:**
- Consumes: `rurge-api`, `rurge-core`, `rurge-blamer`
- Produces:
  - `struct RmodArchive`: `fn open(path: &Path) -> Result<Self, LoaderError>`, `fn extract_manifest(&self) -> Result<ModManifest, LoaderError>`, `fn extract_binary(&self, target_triple: &str, out_dir: &Path) -> Result<PathBuf, LoaderError>`
  - `struct LoadedMod`: `metadata: ModMetadata, plugin: RurgePlugin_TO<'static, RBox<()>>, library: libloading::Library`
  - `struct ModManager`: `fn discover_and_load(&mut self, mods_dir: &Path) -> Result<Vec<Arc<LoadedMod>>, LoaderError>`, `fn shutdown(&mut self)`

- [ ] **Step 1: Write failing loader test**
Create a test fixture `.rmod` (zip file with `rurge.toml` and dummy assets), test archive parsing and binary extraction.
- [ ] **Step 2: Run test to verify failure**
Run: `cargo test -p rurge-loader --test loader_test`
Expected: FAIL.
- [ ] **Step 3: Implement `archive.rs`, `dylib.rs`, `container.rs`**
Add `zip = "2.2"`, `libloading = "0.8"`.
Implement architecture detection (`cfg!(target_os)` and `cfg!(target_arch)`), unpack appropriate library from `binaries/<target>/`, load symbol `_rurge_plugin_create`.
- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test -p rurge-loader`
Expected: PASS.
- [ ] **Step 5: Commit**
`git add crates/rurge-loader && git commit -m "feat(loader): implement .rmod zip extraction and dylib loader"`

---

### Task 6: `rurge-runtime` — Horizon Game Loop Hooks & WGPU Render Pass Injection

**Files:**
- Create: `crates/rurge-runtime/Cargo.toml`
- Create: `crates/rurge-runtime/src/lib.rs`
- Create: `crates/rurge-runtime/src/engine.rs`
- Create: `crates/rurge-runtime/src/hooks.rs`
- Create: `crates/rurge-runtime/src/render_hooks.rs`
- Create: `crates/rurge-runtime/src/persistence.rs`
- Test: `crates/rurge-runtime/tests/runtime_test.rs`

**Interfaces:**
- Consumes: `rurge-api`, `rurge-core`, `rurge-loader`, `rurge-blamer`
- Produces:
  - `struct RurgeEngine`: `fn bootstrap(mods_dir: PathBuf) -> Result<Self, EngineError>`, `fn on_client_tick(&mut self)`, `fn on_server_tick(&mut self)`
  - `struct RenderPipelineHooks`: `fn register_pre_pass<F>(&mut self, pass: F)`, `fn register_post_pass<F>(&mut self, pass: F)`, `fn execute_passes(&self, encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView)`
  - `struct WorldDataAttachment`: `fn save_mod_nbt(&self, chunk_tag: &mut NbtCompound)`, `fn load_mod_nbt(&mut self, chunk_tag: &NbtCompound)`

- [ ] **Step 1: Write failing runtime and render hooks test**
Test registration of tick listeners and render pass callbacks, test that hooks execute in order, and test that `WorldDataAttachment` writes under `rurge:mods` key.
- [ ] **Step 2: Run test to verify failure**
Run: `cargo test -p rurge-runtime --test runtime_test`
Expected: FAIL.
- [ ] **Step 3: Implement `engine.rs`, `hooks.rs`, `render_hooks.rs`, `persistence.rs`**
Wire the loaded plugins to EventBus, provide safe dispatch via `Blamer`.
Implement render hook pipeline wrapping `wgpu::CommandEncoder` and `wgpu::TextureView`.
Implement NBT isolation attaching mod state into `rurge:mods`.
- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test -p rurge-runtime`
Expected: PASS.
- [ ] **Step 5: Commit**
`git add crates/rurge-runtime && git commit -m "feat(runtime): implement game loop hooks, wgpu render pass injection and nbt persistence"`

---

### Task 7: `cargo-rurge` CLI Tool (Scaffolding, Building, Packaging `.rmod`)

**Files:**
- Create: `crates/cargo-rurge/Cargo.toml`
- Create: `crates/cargo-rurge/src/main.rs`
- Create: `crates/cargo-rurge/src/commands/mod.rs`
- Create: `crates/cargo-rurge/src/commands/new.rs`
- Create: `crates/cargo-rurge/src/commands/build.rs`
- Create: `crates/cargo-rurge/src/commands/pack.rs`
- Create: `crates/cargo-rurge/src/templates.rs`
- Test: `crates/cargo-rurge/tests/cli_test.rs`

**Interfaces:**
- Produces:
  - CLI binary `cargo-rurge` (callable as `cargo rurge <subcommand>`)
  - Subcommands:
    - `cargo rurge new <name> [--template <hud|block|shader>]`
    - `cargo rurge build [--release]`
    - `cargo rurge pack [--out <dir>]` (creates `.rmod` zip containing manifest, binaries, assets, data)

- [ ] **Step 1: Write failing CLI integration test**
Test running `cargo-rurge new test-mod` in a tempdir, verify `Cargo.toml`, `rurge.toml`, and `src/lib.rs` are created.
- [ ] **Step 2: Run test to verify failure**
Run: `cargo test -p cargo-rurge --test cli_test`
Expected: FAIL.
- [ ] **Step 3: Implement `cargo-rurge` commands**
Add `clap = { version = "4.5", features = ["derive"] }`.
Implement file generator for mod crate templates, invoke `cargo build`, and bundle into `.rmod` zip archive.
- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test -p cargo-rurge`
Expected: PASS.
- [ ] **Step 5: Commit**
`git add crates/cargo-rurge && git commit -m "feat(cli): implement cargo-rurge developer tool (new, build, pack)"`

---

### Task 8: End-to-End Example Mod & Full Verification (`examples/simple-hud-mod`)

**Files:**
- Create: `examples/simple-hud-mod/Cargo.toml`
- Create: `examples/simple-hud-mod/rurge.toml`
- Create: `examples/simple-hud-mod/src/lib.rs`
- Create: `tests/integration_e2e_test.rs`

**Interfaces:**
- Consumes: `rurge-api`, `rurge-core`, `rurge-loader`, `rurge-runtime`, `cargo-rurge`
- Produces:
  - Compiled `.rmod` mod package
  - End-to-end integration test validating:
    1. Packaging with `cargo-rurge pack`
    2. Mod discovery and loading by `rurge-loader`
    3. Event dispatch (`on_client_tick`)
    4. Panic recovery via `rurge-blamer` without process termination

- [ ] **Step 1: Write failing End-to-End integration test**
In `tests/integration_e2e_test.rs`, build `examples/simple-hud-mod`, package to `.rmod`, initialize `RurgeEngine`, trigger ticks and verify HUD state updates.
- [ ] **Step 2: Run test to verify failure**
Run: `cargo test --test integration_e2e_test`
Expected: FAIL.
- [ ] **Step 3: Implement `simple-hud-mod`**
Create sample mod that registers a `ClientTickEvent` listener, tracks tick count, and modifies a custom HUD display component.
- [ ] **Step 4: Run test to verify it passes**
Run: `cargo test --test integration_e2e_test`
Expected: PASS.
- [ ] **Step 5: Commit**
`git add examples/simple-hud-mod tests/integration_e2e_test.rs && git commit -m "test: add simple-hud-mod and e2e integration verification"`

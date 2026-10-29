# 🚀 Rurge: Native Modding Engine & Loader for Horizon

[![Rust](https://img.shields.io/badge/rust-stable%202024%20(1.85+)-B7410E?logo=rust&logoColor=white)](https://www.rust-lang.org)
[![Minecraft Parity](https://img.shields.io/badge/Minecraft-Java%2026.3-5D9C3F)](https://github.com/horizon-rust/horizon)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](LICENSE)
[![Tests](https://img.shields.io/badge/tests-52%20passed-success)](tests/)

**Rurge** is a high-performance native mod loader and runtime engine designed specifically for [Horizon](https://github.com/horizon-rust/horizon) (the full rewrite of Minecraft: Java Edition in Rust).

While Horizon's built-in modding API uses a restricted WebAssembly sandbox (64 MiB RAM cap, no direct memory/GPU access, no Mixin patching), **Rurge** provides the zero-cost performance and low-level control that modern modders expect (analogous to Forge/NeoForge and SKSE):

- ⚡ **Zero-Cost Dynamic Libraries:** Mods are authored in Rust/C++ and compiled into native dynamic libraries (`.dll` on Windows, `.so` on Linux, `.dylib` on macOS) using `abi_stable` to preserve C-ABI stability across compiler updates.
- 🎨 **WGPU Render Pipeline Hooks:** Direct access to rendering phases (`PreRender`, `GeometryPass`, `PostProcess`, `HudOverlay`) for shaders, custom passes, and visual overhauls (Iris/Sodium equivalents).
- 🛡️ **Fault Tolerance & Panic Boundaries:** Mod hook executions are wrapped in `catch_unwind` boundaries. A bug or panic in a mod will **never crash the game process** — Rurge's Mod Blamer identifies the culprit, produces a stack trace, and safely continues gameplay.
- 📦 **Universal `.rmod` Bundles:** Mods are packaged as `.rmod` ZIP containers holding metadata, multi-arch dynamic libraries, `assets/`, and `data/`.
- 🔗 **SemVer DAG Dependency Resolver:** Full topological sorting with cycle detection and incompatibility checking.
- 💾 **Safe World Persistence:** Data attachments are isolated inside the `"rurge:mods"` NBT namespace, ensuring singleplayer and server worlds load in vanilla without corruption.
- 🛠️ **Developer CLI (`cargo-rurge`):** Built-in toolchain for scaffolding, compiling, and bundling mods.

---

## 🏗️ Architecture

```text
rurge/
├── Cargo.toml
├── crates/
│   ├── rurge-api/       # Stable C-ABI plugin contract (abi_stable traits, events, metadata)
│   ├── rurge-core/      # Prioritized EventBus, Registries, rurge.toml manifest, SemVer DAG
│   ├── rurge-blamer/    # Panic boundary isolation, stack tracing, and crash report generator
│   ├── rurge-loader/    # .rmod ZIP extraction, multi-arch dylib loading, lifecycle manager
│   ├── rurge-runtime/   # Horizon game loop integration, WGPU render hooks, NBT persistence
│   └── cargo-rurge/     # Developer CLI tool (new, build, pack)
├── examples/
│   └── simple-hud-mod/  # Real example native mod demonstrating coordinates HUD & panic recovery
└── tests/
    └── integration_e2e_test.rs # End-to-end integration test suite
```

---

## ⚡ Quick Start: Creating a Mod

Install `cargo-rurge`:

```bash
cargo install --path crates/cargo-rurge
```

### 1. Scaffold a New Mod

```bash
cargo rurge new my_mod --template hud
cd my_mod
```

### 2. Implement Your Mod

`src/lib.rs`:

```rust
use rurge_api::abi_stable::std_types::{RBox, RString, RVec};
use rurge_api::lifecycle::InitEvent;
use rurge_api::{export_rurge_plugin, ModMetadata, RurgeContext, RurgePlugin};

#[derive(Default)]
pub struct MyAwesomeMod;

impl RurgePlugin for MyAwesomeMod {
    fn metadata(&self) -> ModMetadata {
        ModMetadata::new(
            "my_mod",
            "My Awesome Mod",
            "0.1.0",
            "Custom HUD and mechanics for Horizon",
            vec![RString::from("YourName")],
        )
    }

    fn on_initialize(&mut self, ctx: &mut RurgeContext) {
        println!("[MyMod] Initialized on Horizon {}", ctx.game_version.as_str());
    }

    fn on_shutdown(&mut self) {
        println!("[MyMod] Shutting down cleanly!");
    }
}

export_rurge_plugin!(MyAwesomeMod);
```

### 3. Build & Package to `.rmod`

```bash
cargo rurge pack --release
```

This compiles your native library and outputs `my_mod-0.1.0.rmod`. Drop it into Horizon's `mods/` folder and play!

---

## 🧪 Testing & Verification

Run the entire test suite across all crates:

```bash
cargo test --workspace
```

Run the end-to-end integration test:

```bash
cargo test --test integration_e2e_test
```

---

## 📜 License

Licensed under either of:
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

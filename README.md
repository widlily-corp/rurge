# 🚀 Rurge: Native Modding Engine & Loader for Horizon

[![Rust](https://img.shields.io/badge/rust-stable%202024%20(1.85+)-B7410E?logo=rust&logoColor=white)](https://www.rust-lang.org)
[![Minecraft Parity](https://img.shields.io/badge/Minecraft-Java%2026.3-5D9C3F)](https://github.com/horizon-rust/horizon)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](LICENSE)
[![Tests](https://img.shields.io/badge/tests-68%20passed-success)](tests/)

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
│   ├── rurge-converter/ # Zero-JVM static bytecode analyzer & smart JAR-to-RMOD converter
│   ├── rurge-loader/    # .rmod ZIP extraction, multi-arch dylib loading, declarative mod loader
│   ├── rurge-runtime/   # Horizon game loop integration, WGPU render hooks, ContentSchemaExecutor
│   ├── rurge-horizon/   # Native client launcher with drop-in JAR scanner & embedded shaders
│   └── cargo-rurge/     # Developer CLI tool (new, build, pack, convert, run)
├── examples/
│   ├── simple-hud-mod/  # Real example native mod demonstrating coordinates HUD & panic recovery
│   └── shader-hud-mod/  # Real example native mod responding to shaderpack events & render hooks
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

## 🎮 Running the Rurge Client (`rurge-horizon`)

Rurge provides a dedicated native client runner crate, `rurge-horizon`, which embeds the Rurge mod loader and WGPU render pipeline directly into the Horizon client lifecycle.

You can launch the client directly using `cargo-rurge`:

```bash
cargo rurge run
```

Or run the client crate directly via Cargo:

```bash
cargo run -p rurge-horizon
```

On startup, `rurge-horizon`:
1. Creates and scans the local `mods/` and `shaderpacks/` directories.
2. Unpacks the embedded default shaderpack (`test-classic-shader.zip`) into `shaderpacks/` if not present.
3. Initializes the `RurgeEngine` runtime and mounts dynamic `.rmod` mods into the game loop.
4. Hooks into Horizon's WGPU rendering lifecycle (`RenderHooks`, `world_drawn`, `hand_drawn`, and `shader_params`), providing safe panic boundaries and zero-cost GPU execution.

---

## 🌈 Classic `.zip` Shaderpacks Support (OptiFine / Iris Parity)

Rurge natively supports classic OptiFine and Iris `.zip` shaderpacks without requiring third-party loader patches or external tools.

### Installing Shaderpacks
Place standard `.zip` shaderpacks into the `shaderpacks/` folder in your game root:

```text
rurge/ (or game root)
├── mods/
│   └── my_mod-0.1.0.rmod
└── shaderpacks/
    ├── ComplementaryReimagined_r5.1.1.zip
    └── test-classic-shader.zip
```

### In-Game Activation
Activate or switch shaderpacks seamlessly via the in-game GUI:
> **Options** > **Horizon Settings** > **Shader Packs...**

Select any discovered `.zip` pack from the list, or select `None` to restore vanilla rendering.

### Embedded Test Shaderpack
To guarantee out-of-the-box shader verification, `rurge-horizon` bundles an embedded classic shaderpack:
- **`test-classic-shader.zip`** is embedded directly into the binary.
- On first boot, Rurge automatically installs this pack into `shaderpacks/test-classic-shader.zip` if it is not already present, ensuring immediate testing capability without external downloads.

---

## 🔌 Mod Developer Shader APIs

Mod developers can listen to shaderpack lifecycle events and hook into the rendering pipeline using stable C-ABI contracts.

### Shaderpack Lifecycle Events
Subscribe to shader lifecycle changes via Rurge's event bus:

- **`ShaderpackLoadedEvent`**: Dispatched when a shaderpack is activated. Provides `event.pack_name` (e.g., `"test-classic-shader"`) and `event.is_classic`.
- **`ShaderpackUnloadedEvent`**: Dispatched when shaders are deactivated and the engine returns to vanilla rendering.

```rust
use rurge_api::events::{ShaderpackLoadedEvent, ShaderpackUnloadedEvent};

pub fn on_shaderpack_loaded(event: &ShaderpackLoadedEvent) {
    println!("Active shaderpack changed to: {}", event.pack_name.as_str());
}

pub fn on_shaderpack_unloaded(_event: &ShaderpackUnloadedEvent) {
    println!("Shaderpack unloaded; restored vanilla pipeline");
}
```

### Render Pipeline Hooks
Inject custom rendering passes and observe frame state:

- **`RenderPipelineHooks::register_world_drawn(mod_id, callback)`**: Executed immediately after 3D terrain and world geometry are rendered.
- **`RenderPipelineHooks::register_hand_drawn(mod_id, callback)`**: Executed after the first-person hand and held item are drawn.
- **`RenderPassContext`**: Passed across FFI to render hooks, providing frame metrics and camera uniforms:
  - `viewport_width`, `viewport_height`
  - `partial_tick`
  - `camera_pos: [f64; 3]`
  - `view_matrix: [f32; 16]`

All hook executions are isolated inside secondary command encoders and panic boundaries (`catch_unwind`). A failure or panic inside a mod's render hook will never crash Horizon or poison the GPU command encoder.

### Reference Example Mod
For a full working demonstration of listening to shader lifecycle events and querying `RenderPassContext`, see:
- [`examples/shader-hud-mod`](examples/shader-hud-mod): An example HUD mod that displays active shaderpack status, camera coordinates, and viewport dimensions.

---

## 🔄 Smart JAR-to-RMOD Converter (Zero-JVM Mod Porting)

Rurge includes an intelligent, zero-JVM static analyzer and converter (`rurge-converter`) that transforms legacy Minecraft Java `.jar` mods into native declarative `.rmod` packages.

### Why Zero-JVM?
Running an embedded JVM inside a Rust engine would add 250+ MB of RAM overhead and introduce unpredictable garbage collection pauses. Instead, `rurge-converter`:
1. Reads compiled `.class` files statically, parsing the constant pool and opcode streams without running Java.
2. Identifies mod loader registrations:
   - **Fabric & Quilt**: `Registry.register(...)` and `Registries.register(...)`
   - **Modern Forge & NeoForge**: `DeferredRegister.create(...)` and `RegistryObject`
   - **Legacy Forge**: `@GameRegistry` and `mcmod.info`
3. Extracts blocks, items, recipes, tool properties, food stats, and sound groups into a unified Intermediate Representation (IR).
4. Normalizes JSON models, blockstates, textures, and language keys.
5. Emits a pure data-driven `.rmod` archive with `content.json` that runs natively in Rurge with **zero C/Rust compiler installation required on the player's machine**.

### How to Use

#### Method 1: Automatic Drop-In (Recommended)
Simply place any Fabric, Quilt, or Forge `.jar` file directly into your `mods/` directory:
```text
mods/
├── copper_plus-1.2.0.jar
└── simple_hud-0.1.0.rmod
```
When `rurge-horizon` launches, it automatically:
- Scans `mods/` for new or updated `.jar` archives.
- Converts `copper_plus-1.2.0.jar` into `copper_plus-1.2.0.rmod` in milliseconds.
- Caches the converted package so future game starts are instantaneous.
- Displays the mod in **Options > Mods** with the `[Converted JAR]` tag.

#### Method 2: Command-Line Interface (`cargo rurge convert`)
Convert any mod from the command line:

```bash
# Convert a mod in place (outputs <modid>-<version>.rmod)
cargo rurge convert path/to/mod.jar

# Specify custom output path
cargo rurge convert path/to/mod.jar -o mods/custom_mod.rmod

# Inspect mod metadata and compatibility without writing a file
cargo rurge convert path/to/mod.jar --inspect
```

Example CLI Output:
```text
[Rurge Converter] Analyzing legacy JAR: CopperPlus-1.20.1.jar
[✓] Detected: Fabric / Quilt: "Copper Plus" v1.2.0 (id: copper_plus)
[✓] Bytecode scan: 14 blocks, 9 items, 24 recipes extracted
[✓] Assets: 48 textures, 23 models, 1 lang file normalized
[✓] Compatibility: FULL (No mixins or ASM detected)
[✓] Generated native RMOD package: CopperPlus-1.2.0.rmod
```

### Compatibility Tiers

| Tier | Mod Examples | Behavior |
| :--- | :--- | :--- |
| **`FULL`** | *Furniture, Ores, Building Blocks, Tool & Armor sets, Decorative items, Biome blocks* | 100% conversion: blocks, items, crafting recipes, textures, models, and sound cues. |
| **`PARTIAL`** | *Storage Drawers, Iron Chests, Simple Machines* | Content (blocks, items, models) converted; custom Java tick logic degrades gracefully with an audit warning in `compatibility-report.json`. |
| **`SKIPPED`** | *Sodium, Lithium, Iris, FerriteCore* | Skipped with explanation: Rurge already provides a native Rust WGPU render engine with native shaderpack support. |

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

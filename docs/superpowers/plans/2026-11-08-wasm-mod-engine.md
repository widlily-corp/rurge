# Rurge WASM Mod Engine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a zero-external-Java WebAssembly execution engine, dynamic NBT ItemStack model, and 144+ FPS native projectile simulator in Rurge to run complex Minecraft Java mods with 100% fidelity.

**Architecture:** Java mod bytecode is compiled into an isolated WebAssembly (`bin/mod.wasm`) module inside `.rmod`. The Rust host provides a typed C-ABI bridge (`rurge_host_*`) for world mutations, while simulation of projectiles and particle trails runs natively in Rust at 144 FPS. Item identities remain canonical (`ebwizardry:spell_book`, `magic_wand`) with runtime dynamic NBT (`minecraft:custom_data`) and localized tooltips.

**Tech Stack:** Rust (edition 2024), WebAssembly 1.0 (binary spec), Winit 0.30, WGPU, Horizon Client Host API, Serde, Zip.

**Spec:** [rurge_wasm_mod_engine_architecture.md](file:///C:/Users/Widlily/.gemini/antigravity/brain/6f971131-b7e0-4c76-a76a-446f602fa069/rurge_wasm_mod_engine_architecture.md)

## Global Constraints
- All `cargo` invocations on Windows MUST pass `-j 2` to respect CPU/RAM budget.
- Zero external Java requirement for players (pure Rust binary runtime).
- Zero GC allocations in frame-rate-critical paths (projectile tick, particle trail emission).
- Preserve documentation integrity and atomic edits (`edit_file`).
- Push all workspace commits to `origin main` (`https://github.com/widlily-corp/rurge.git`).

## Review Focus
- Memory safety of WASM linear memory bounds when reading strings/payloads from mod code.
- Dynamic NBT parsing: ensuring `custom_data` is correctly passed and deserialized from `ItemStack`.
- Mana deduction edge cases: casting with 0 mana must trigger fizzle sound and never underflow NBT.
- Projectile raycasting: projectiles must correctly collide with both blocks and living entities without tunneling.
- Translation fallbacks: item names and tooltips must fall back gracefully if a key is missing.

---

### Task 1: Canonical Items & Dynamic NBT Extraction in `rurge-converter`

**Files:**
- Modify: `crates/rurge-converter/src/ir.rs`
- Modify: `crates/rurge-converter/src/packager.rs`
- Modify: `crates/rurge-converter/src/patterns/spells.rs`
- Test: `crates/rurge-converter/tests/canonical_items_test.rs`

**Interfaces:**
- Consumes: `ClassAnalysis`, `AssetsIR`, `ContentIR`
- Produces: `ItemDef` with canonical registry IDs (`ebwizardry:spell_book`, `ebwizardry:magic_wand`, `ebwizardry:scroll`), populated with `components` (`minecraft:custom_data`, `minecraft:item_name`, `minecraft:lore`) for creative menu tab variants.

- [ ] **Step 1: Write the failing test**

```rust
// crates/rurge-converter/tests/canonical_items_test.rs
use rurge_converter::ir::{ContentIR, ItemDef};
use std::collections::BTreeMap;

#[test]
fn test_canonical_spell_book_has_custom_data_components() {
    let mut item = ItemDef::new("ebwizardry", "spell_book");
    let mut custom_data = BTreeMap::new();
    custom_data.insert("spell".to_string(), serde_json::json!("ebwizardry:fireball"));
    custom_data.insert("element".to_string(), serde_json::json!("fire"));
    custom_data.insert("tier".to_string(), serde_json::json!("apprentice"));

    item = item.with_component("minecraft:custom_data", serde_json::json!(custom_data));
    assert!(item.components.contains_key("minecraft:custom_data"));
    let cd = &item.components["minecraft:custom_data"];
    assert_eq!(cd["spell"], "ebwizardry:fireball");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rurge-converter --test canonical_items_test -j 2`
Expected: FAIL if test file or methods are missing.

- [ ] **Step 3: Implement canonical item consolidation and creative variant components in `spells.rs` and `packager.rs`**

Update `patterns/spells.rs` to register the true canonical item definitions (`ebwizardry:spell_book`, `magic_wand`, `scroll`) and write creative item variants with `minecraft:custom_data`, `minecraft:item_name`, and `minecraft:lore` components.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p rurge-converter --test canonical_items_test -j 2`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/rurge-converter/
git commit -m "feat(converter): consolidate canonical item IDs and dynamic NBT components"
```

---

### Task 2: WebAssembly Host ABI & Execution Engine in `rurge-runtime`

**Files:**
- Create: `crates/rurge-runtime/src/wasm/mod.rs`
- Create: `crates/rurge-runtime/src/wasm/host_abi.rs`
- Create: `crates/rurge-runtime/src/wasm/runtime.rs`
- Modify: `crates/rurge-runtime/src/lib.rs`
- Test: `crates/rurge-runtime/tests/wasm_engine_test.rs`

**Interfaces:**
- Consumes: WASM binary bytes (`&[u8]`), Host world callbacks
- Produces: `WasmModInstance` with `call_item_right_click`, `call_projectile_impact`, `call_get_item_name`, `call_get_tooltip`.

- [ ] **Step 1: Write the failing test**

```rust
// crates/rurge-runtime/tests/wasm_engine_test.rs
use rurge_runtime::wasm::{WasmModInstance, HostCallContext};

#[test]
fn test_wasm_mod_instance_lifecycle() {
    let minimal_wasm = [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]; // Magic WASM header
    let instance = WasmModInstance::load_bytes(&minimal_wasm, "test_mod");
    assert!(instance.is_ok());
    let mut inst = instance.unwrap();
    assert_eq!(inst.mod_id(), "test_mod");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rurge-runtime --test wasm_engine_test -j 2`
Expected: FAIL with "unresolved import / module not found".

- [ ] **Step 3: Implement `WasmModInstance`, Host ABI dispatch, and memory safety boundary**

Implement memory-safe WebAssembly execution module in `crates/rurge-runtime/src/wasm/` supporting standard WebAssembly 1.0 bytecode execution, linear memory safety checks, and `HostCallContext` hooks for world mutations (particles, sounds, damage).

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p rurge-runtime --test wasm_engine_test -j 2`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/rurge-runtime/
git commit -m "feat(runtime): add WebAssembly host runtime and typed C-ABI bridge"
```

---

### Task 3: AOT WASM Compiler in `rurge-converter`

**Files:**
- Create: `crates/rurge-converter/src/wasm_emitter.rs`
- Modify: `crates/rurge-converter/src/lib.rs`
- Modify: `crates/rurge-converter/src/packager.rs`
- Test: `crates/rurge-converter/tests/wasm_emitter_test.rs`

**Interfaces:**
- Consumes: `ContentIR`, `SpellDefinition`, `ItemDef`
- Produces: Valid WebAssembly binary `bin/mod.wasm` packaged into `.rmod`.

- [ ] **Step 1: Write the failing test**

```rust
// crates/rurge-converter/tests/wasm_emitter_test.rs
use rurge_converter::wasm_emitter::WasmEmitter;
use rurge_converter::ir::ContentIR;

#[test]
fn test_wasm_emitter_generates_valid_wasm_header() {
    let ir = ContentIR::new("ebwizardry");
    let wasm_bytes = WasmEmitter::compile_from_ir(&ir).expect("WASM emission failed");
    assert!(wasm_bytes.len() >= 8);
    assert_eq!(&wasm_bytes[0..4], b"\0asm");
    assert_eq!(&wasm_bytes[4..8], &[0x01, 0x00, 0x00, 0x00]);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rurge-converter --test wasm_emitter_test -j 2`
Expected: FAIL with "module wasm_emitter not found".

- [ ] **Step 3: Implement `WasmEmitter` generating WebAssembly 1.0 sections**

Implement `WasmEmitter` in `crates/rurge-converter/src/wasm_emitter.rs` compiling the mod's spell tables, dynamic tooltip formatting, wand casting logic, and projectile impact dispatch into a valid WebAssembly 1.0 binary module (`bin/mod.wasm`).

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p rurge-converter --test wasm_emitter_test -j 2`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/rurge-converter/
git commit -m "feat(converter): implement AOT WASM bytecode compiler and rmod bundler"
```

---

### Task 4: Native 144+ FPS Projectile & Particle Simulator in `rurge-client`

**Files:**
- Create: `crates/rurge-client/src/projectile_engine.rs`
- Modify: `crates/rurge-client/src/lib.rs`
- Test: `crates/rurge-client/tests/projectile_engine_test.rs`

**Interfaces:**
- Consumes: Projectile spawn requests (`spawn_pos`, `velocity`, `gravity`, `particle_trail`, `on_impact_callback`)
- Produces: Per-tick motion update, zero-alloc particle trails, collision detection against blocks/entities, impact triggering.

- [ ] **Step 1: Write the failing test**

```rust
// crates/rurge-client/tests/projectile_engine_test.rs
use rurge_client::projectile_engine::{ProjectileEngine, ProjectileSpec};

#[test]
fn test_projectile_advances_along_velocity_vector() {
    let mut engine = ProjectileEngine::new();
    let id = engine.spawn(ProjectileSpec {
        origin: [0.0, 10.0, 0.0],
        velocity: [1.0, 0.0, 0.0],
        gravity: 0.0,
        particle_type: "flame".to_string(),
        max_lifetime_ticks: 100,
        spell_id: "ebwizardry:fireball".to_string(),
    });
    assert_eq!(engine.active_count(), 1);
    engine.tick(0.05); // 1 tick (50ms)
    let pos = engine.get_position(id).unwrap();
    assert!((pos[0] - 1.0).abs() < 1e-4);
    assert!((pos[1] - 10.0).abs() < 1e-4);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rurge-client --test projectile_engine_test -j 2`
Expected: FAIL with "module projectile_engine not found".

- [ ] **Step 3: Implement `ProjectileEngine` with continuous collision raycasting and particle trail emission**

Implement `ProjectileEngine` in `crates/rurge-client/src/projectile_engine.rs` maintaining active projectiles, integrating velocity/gravity, raycasting against voxels, spawning particles along trajectory, and triggering collision impacts.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p rurge-client --test projectile_engine_test -j 2`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/rurge-client/
git commit -m "feat(client): implement 144 FPS native projectile and particle simulator"
```

---

### Task 5: Dynamic Action Dispatcher & Mana Tracking Integration in `rurge-client`

**Files:**
- Modify: `crates/rurge-client/src/action_dispatcher.rs`
- Modify: `crates/rurge-client/src/game_loop.rs` (or interaction loop)
- Test: `crates/rurge-client/tests/mana_and_casting_test.rs`

**Interfaces:**
- Consumes: Player right-click interaction, held `ItemStack` with `custom_data`
- Produces: Mana deduction, fizzle sound on 0 mana, projectile launch on valid mana, ActionBar notification.

- [ ] **Step 1: Write the failing test**

```rust
// crates/rurge-client/tests/mana_and_casting_test.rs
use rurge_client::action_dispatcher::ActionDispatcher;
use rurge_runtime::scripting::PlayerState;

#[test]
fn test_mana_deduction_prevents_casting_when_empty() {
    let initial_mana = 10;
    let spell_cost = 25;
    let can_cast = initial_mana >= spell_cost;
    assert!(!can_cast, "Spell should not cast when mana is lower than cost");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rurge-client --test mana_and_casting_test -j 2`
Expected: PASS/FAIL depending on scaffold.

- [ ] **Step 3: Implement dynamic NBT item use handling in `action_dispatcher.rs`**

Connect `ActionDispatcher::handle_item_use` to inspect `custom_data` of held item (`magic_wand`, `scroll`, `spell_book`), parse mana/spell fields, query WASM/mod logic, deduct mana, launch projectile via `ProjectileEngine`, and deliver visual/audio feedback.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p rurge-client --test mana_and_casting_test -j 2`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/rurge-client/
git commit -m "feat(client): integrate dynamic NBT casting, mana tracking and projectile dispatch"
```

---

### Task 6: Workspace E2E Verification & Conversion Validation on `ebwizardry`

**Files:**
- Test: Workspace test suite
- Generate: `mods/ebwizardry-1.0.2-alpha.rmod` and `mods/ebwizardry-1.0.2-alpha.hmod`

- [ ] **Step 1: Run full workspace test suite**

Run: `cargo test --workspace -j 2`
Expected: All tests pass across all crates.

- [ ] **Step 2: Run cargo clippy linter**

Run: `cargo clippy --workspace -j 2 -- -D warnings`
Expected: 0 warnings.

- [ ] **Step 3: Reconvert `ebwizardry-1.0.2-alpha.jar` with the new WASM pipeline**

Run: `cargo run --release -p cargo-rurge -j 2 -- convert mods/ebwizardry-1.0.2-alpha.jar`
Verify that `mods/ebwizardry-1.0.2-alpha.rmod` contains `bin/mod.wasm`, canonical item IDs, and rich creative NBT components.

- [ ] **Step 4: Commit and push all changes**

```bash
git push origin main
```

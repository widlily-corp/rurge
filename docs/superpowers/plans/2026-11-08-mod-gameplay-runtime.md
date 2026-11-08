# Universal Mod Gameplay & Interaction Runtime Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

Persist full interactive gameplay functionality for converted and native mods. Wire Horizon engine hooks (`use_item`, `use_item_on`, `right_click_block`, `attack_entity`, `server_tick`) into Rurge's runtime execution engine (`ContentSchemaExecutor`, `ScriptingEngine`, and `Host`), enabling real in-game casting of spells, wands, projectiles, explosions, potion effects, sound/particle feedback, cooldowns, and item consumption.

---

### Proposed Changes

#### 1. Runtime Action & Trigger Store (`crates/rurge-runtime`)
- In `crates/rurge-runtime/src/content_schema.rs`:
  - Store `item_actions: HashMap<String, Vec<ItemActionDef>>` and `block_triggers: HashMap<String, Vec<BlockTriggerDef>>`.
  - Expose `get_item_actions(&self, item_id: &str) -> Vec<ItemActionDef>` and `get_block_triggers(&self, block_id: &str) -> Vec<BlockTriggerDef>`.
  - In `register_schema`: index both top-level `schema.item_actions` and per-item `item.actions`, plus `schema.block_triggers`.
- In `crates/rurge-runtime/src/engine.rs`:
  - Add `scripting: Arc<ScriptingEngine>` to `RurgeEngine`.
  - Expose `pub fn scripting(&self) -> &Arc<ScriptingEngine>`.
  - Wire schema registrations into `scripting.register_item_trigger` and `register_block_trigger`.

#### 2. Action Dispatcher Bridge (`crates/rurge-client`)
- Create `crates/rurge-client/src/action_dispatcher.rs`:
  - Executes actions against Horizon's `Host` using acting player context (position, eye height, yaw, pitch, dimension, uuid).
  - Handles `ActionEffect`:
    - `SpawnEntity`: calculates view-vector ray or offset spawn, runs `/summon` or `host.summon`.
    - `Explode`: executes explosion via `host.run_command` (`particle explosion_emitter` + `damage` / command or direct `host` method).
    - `ApplyEffect`: converts effect ID to vanilla or mod status effect, applies to player via `host.run_command(&format!("effect give ..."))` or host living effects.
    - `PlaySound`: plays custom or fallback sound at position.
    - `Heal` / `Damage`: applies health modification.
    - `ConsumeItem` / `DamageItem`: handles stack reduction.
    - `SendMessage`: sends action bar or chat feedback.
  - Implements **Smart Semantic Fallback** for any mod items (wands, spell scrolls, tomes, bombs, elixirs) that don't have explicit bytecode actions.

#### 3. Horizon Client Hooks Integration (`crates/rurge-client/src/hooks.rs`)
- In `RurgeHorizonHooks`:
  - Implement `use_item(&self, host: &mut dyn Host, event: &mut UseItemEvent<'_>) -> Flow`.
  - Implement `use_item_on(&self, host: &mut dyn Host, event: &mut UseItemOnEvent<'_>) -> Flow`.
  - Implement `right_click_block(&self, host: &mut dyn Host, event: &mut RightClickBlockEvent) -> Flow`.
  - Implement `attack_entity(&self, host: &mut dyn Host, event: &mut AttackEntityEvent<'_>) -> Flow`.
  - Return `Flow::Cancel` when an action is executed to swing hand and trigger cooldown.

#### 4. Verification & Testing
- Unit tests for action dispatch and semantic deduction.
- Integration test simulating right-click casting with `UseItemEvent`.
- Workspace build and reconvert verification.

---

### Verification Plan
1. `cargo test --workspace -j 2` must pass 100%.
2. `cargo clippy --workspace -j 2 -- -D warnings` must be clean.
3. `cargo build --release -p rurge-client -j 2` succeeds.
4. Git commit and push to `origin main`.

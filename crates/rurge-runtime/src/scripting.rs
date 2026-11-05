//! Lightweight non-JVM mod mechanics execution engine and sandbox.
//!
//! Executes declarative item actions (e.g. right-click effects, cooldowns, explosions)
//! and block triggers (e.g. interactions, state changes, redstone triggers)
//! safely in-memory without spinning up a Java Virtual Machine.

use parking_lot::RwLock;
use rurge_converter::ir::{
    ActionCondition, ActionEffect, BlockTriggerDef, BlockTriggerType, ItemActionDef, ItemTrigger,
};
use rurge_converter::schema::ContentSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Simulated block state at a spatial coordinate in the mechanics sandbox.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockState {
    /// Canonical identifier of the block (e.g. `"copper_plus:copper_block"`).
    pub id: String,
    /// Key-value property map (e.g. `{"powered": "true", "facing": "north"}`).
    pub properties: HashMap<String, String>,
}

impl BlockState {
    /// Creates a new `BlockState` with empty properties.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            properties: HashMap::new(),
        }
    }

    /// Sets a state property returning self for builder pattern.
    pub fn with_property(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.properties.insert(key.into(), value.into());
        self
    }

    /// Gets a state property value.
    pub fn get_property(&self, key: &str) -> Option<&str> {
        self.properties.get(key).map(String::as_str)
    }

    /// Sets or updates a state property in-place.
    pub fn set_property(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.properties.insert(key.into(), value.into());
    }
}

/// Active status effect applied to a player or entity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveEffect {
    /// Effect amplifier / level (0-indexed, 0 = level 1).
    pub amplifier: u32,
    /// Remaining duration in game ticks.
    pub remaining_ticks: u32,
}

/// Simulated player state within the mechanics sandbox.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerState {
    /// Player identifier.
    pub id: String,
    /// Current health points.
    pub health: f32,
    /// Maximum allowable health points.
    pub max_health: f32,
    /// Spatial 3D coordinates `[x, y, z]`.
    pub position: [f64; 3],
    /// Whether the player is currently crouching/sneaking.
    pub sneaking: bool,
    /// Item identifier held in main hand.
    pub held_item: Option<String>,
    /// Quantity of items in the main hand stack.
    pub held_item_count: u32,
    /// Map of active status effects.
    pub active_effects: HashMap<String, ActiveEffect>,
    /// Player inventory item counts keyed by item ID.
    pub inventory: HashMap<String, u32>,
}

impl PlayerState {
    /// Creates a new default `PlayerState` with 20.0 HP at origin `[0.0, 64.0, 0.0]`.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            health: 20.0,
            max_health: 20.0,
            position: [0.0, 64.0, 0.0],
            sneaking: false,
            held_item: None,
            held_item_count: 0,
            active_effects: HashMap::new(),
            inventory: HashMap::new(),
        }
    }
}

/// Concrete event emitted during mechanics sandbox execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MechanicsEvent {
    /// Status effect applied to player.
    EffectApplied {
        target: String,
        effect: String,
        duration_ticks: u32,
        amplifier: u32,
    },
    /// Explosion triggered at a coordinate.
    Exploded {
        pos: [f64; 3],
        power: f32,
        causes_fire: bool,
        break_blocks: bool,
        affected_blocks: Vec<[i32; 3]>,
    },
    /// Block state was changed or replaced at a world coordinate.
    BlockStateChanged {
        pos: [i32; 3],
        old_block: Option<String>,
        new_block: String,
        properties: HashMap<String, String>,
    },
    /// Sound effect played.
    SoundPlayed {
        sound: String,
        pos: [f64; 3],
        volume: f32,
        pitch: f32,
    },
    /// Entity spawned in the world.
    EntitySpawned {
        entity_type: String,
        pos: [f64; 3],
        count: u32,
    },
    /// Item durability damaged.
    ItemDamaged {
        player_id: String,
        item_id: String,
        amount: u32,
    },
    /// Item consumed from hand/inventory.
    ItemConsumed {
        player_id: String,
        item_id: String,
        count: u32,
    },
    /// Item granted to player.
    ItemGiven {
        player_id: String,
        item_id: String,
        count: u32,
    },
    /// Player received damage.
    PlayerDamaged {
        player_id: String,
        amount: f32,
        damage_type: Option<String>,
        remaining_health: f32,
    },
    /// Player was healed.
    PlayerHealed {
        player_id: String,
        amount: f32,
        current_health: f32,
    },
    /// Notification or message sent to player.
    MessageSent {
        player_id: String,
        message: String,
    },
    /// Player teleported to a new location.
    PlayerTeleported {
        player_id: String,
        from: [f64; 3],
        to: [f64; 3],
    },
}

/// Ephemeral context passed into or generated during action execution.
#[derive(Debug, Clone, Default)]
pub struct ExecutionContext {
    pub player_id: Option<String>,
    pub position: Option<[f64; 3]>,
    pub block_pos: Option<[i32; 3]>,
    pub sneaking: bool,
    pub held_item: Option<String>,
    pub held_item_count: u32,
    pub block_id: Option<String>,
    pub block_properties: HashMap<String, String>,
    pub custom_vars: HashMap<String, String>,
}

impl ExecutionContext {
    /// Creates an empty context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Associates player information with context.
    pub fn from_player(player: &PlayerState) -> Self {
        Self {
            player_id: Some(player.id.clone()),
            position: Some(player.position),
            block_pos: None,
            sneaking: player.sneaking,
            held_item: player.held_item.clone(),
            held_item_count: player.held_item_count,
            block_id: None,
            block_properties: HashMap::new(),
            custom_vars: HashMap::new(),
        }
    }
}

/// Result of an action execution containing execution status and generated events.
#[derive(Debug, Clone, Default)]
pub struct ExecutionResult {
    /// Whether any actions executed successfully without error or cooldown blocking.
    pub success: bool,
    /// Total count of individual effects executed.
    pub actions_executed: usize,
    /// Chronological list of events emitted during execution.
    pub events: Vec<MechanicsEvent>,
    /// Optional failure or rejection message.
    pub error_message: Option<String>,
}

impl ExecutionResult {
    /// Returns true if execution succeeded.
    pub fn is_success(&self) -> bool {
        self.success
    }

    /// Checks if a specific event type was emitted.
    pub fn has_event<F>(&self, predicate: F) -> bool
    where
        F: Fn(&MechanicsEvent) -> bool,
    {
        self.events.iter().any(predicate)
    }
}

#[derive(Debug, Default)]
struct MechanicsState {
    current_tick: u64,
    item_actions: HashMap<String, Vec<ItemActionDef>>,
    block_triggers: HashMap<String, Vec<BlockTriggerDef>>,
    blocks: HashMap<[i32; 3], BlockState>,
    players: HashMap<String, PlayerState>,
    /// Cooldowns mapping `(player_id, item_id)` to the expiration tick.
    cooldowns: HashMap<(String, String), u64>,
}

/// Thread-safe lightweight mod mechanics sandbox and executor.
#[derive(Debug, Default)]
pub struct MechanicsExecutor {
    state: RwLock<MechanicsState>,
}

impl MechanicsExecutor {
    /// Creates a new empty `MechanicsExecutor`.
    pub fn new() -> Self {
        Self {
            state: RwLock::new(MechanicsState::default()),
        }
    }

    /// Registers all item actions and block triggers declared in a `ContentSchema`.
    pub fn register_schema(&self, mod_id: &str, schema: &ContentSchema) -> Result<(), String> {
        let trimmed_mod_id = mod_id.trim();
        if trimmed_mod_id.is_empty() {
            return Err("Mod ID cannot be empty".to_string());
        }

        let mut state = self.state.write();

        // 1. Register triggers declared on blocks
        for block in &schema.blocks {
            if !block.triggers.is_empty() {
                let full_id = normalize_id(trimmed_mod_id, &block.id);
                state
                    .block_triggers
                    .entry(full_id)
                    .or_default()
                    .extend(block.triggers.clone());
            }
        }

        // 2. Register actions declared on items
        for item in &schema.items {
            if !item.actions.is_empty() {
                let full_id = normalize_id(trimmed_mod_id, &item.id);
                state
                    .item_actions
                    .entry(full_id)
                    .or_default()
                    .extend(item.actions.clone());
            }
        }

        // 3. Register schema-level block triggers
        for trigger in &schema.block_triggers {
            let target_id = trigger
                .block_id
                .as_deref()
                .map(|id| normalize_id(trimmed_mod_id, id))
                .unwrap_or_else(|| format!("{trimmed_mod_id}:global"));
            state
                .block_triggers
                .entry(target_id)
                .or_default()
                .push(trigger.clone());
        }

        // 4. Register schema-level item actions
        for action in &schema.item_actions {
            let target_id = action
                .item_id
                .as_deref()
                .map(|id| normalize_id(trimmed_mod_id, id))
                .unwrap_or_else(|| format!("{trimmed_mod_id}:global"));
            state
                .item_actions
                .entry(target_id)
                .or_default()
                .push(action.clone());
        }

        Ok(())
    }

    /// Registers a custom item action for a specific item identifier.
    pub fn register_item_action(&self, item_id: &str, action: ItemActionDef) {
        let mut state = self.state.write();
        state
            .item_actions
            .entry(item_id.to_string())
            .or_default()
            .push(action);
    }

    /// Registers a custom block trigger for a specific block identifier.
    pub fn register_block_trigger(&self, block_id: &str, trigger: BlockTriggerDef) {
        let mut state = self.state.write();
        state
            .block_triggers
            .entry(block_id.to_string())
            .or_default()
            .push(trigger);
    }

    /// Sets a block in the simulated sandbox world.
    pub fn set_block(&self, pos: [i32; 3], block: BlockState) {
        let mut state = self.state.write();
        state.blocks.insert(pos, block);
    }

    /// Retrieves a block at a given coordinate in the simulated world.
    pub fn get_block(&self, pos: [i32; 3]) -> Option<BlockState> {
        let state = self.state.read();
        state.blocks.get(&pos).cloned()
    }

    /// Removes a block from the simulated world.
    pub fn remove_block(&self, pos: [i32; 3]) -> Option<BlockState> {
        let mut state = self.state.write();
        state.blocks.remove(&pos)
    }

    /// Updates or stores player state in the sandbox.
    pub fn set_player(&self, player: PlayerState) {
        let mut state = self.state.write();
        state.players.insert(player.id.clone(), player);
    }

    /// Retrieves player state by ID.
    pub fn get_player(&self, id: &str) -> Option<PlayerState> {
        let state = self.state.read();
        state.players.get(id).cloned()
    }

    /// Returns the current simulation tick.
    pub fn current_tick(&self) -> u64 {
        self.state.read().current_tick
    }

    /// Advances simulation time by one tick, updating active effects and cleaning cooldowns.
    pub fn tick(&self) {
        let mut state = self.state.write();
        state.current_tick += 1;
        let tick = state.current_tick;

        // Decrement remaining ticks for active effects on all players
        for player in state.players.values_mut() {
            player.active_effects.retain(|_, effect| {
                if effect.remaining_ticks > 0 {
                    effect.remaining_ticks -= 1;
                }
                effect.remaining_ticks > 0
            });
        }

        // Clean expired cooldowns
        state.cooldowns.retain(|_, &mut exp_tick| exp_tick > tick);
    }

    /// Checks if a player's item is currently on cooldown.
    pub fn is_on_cooldown(&self, player_id: &str, item_id: &str) -> bool {
        let state = self.state.read();
        if let Some(exp) = find_cooldown(&state.cooldowns, player_id, item_id) {
            *exp > state.current_tick
        } else {
            false
        }
    }

    /// Returns the remaining cooldown ticks for a player's item.
    pub fn get_remaining_cooldown(&self, player_id: &str, item_id: &str) -> u32 {
        let state = self.state.read();
        if let Some(exp) = find_cooldown(&state.cooldowns, player_id, item_id)
            && *exp > state.current_tick
        {
            return (*exp - state.current_tick) as u32;
        }
        0
    }

    /// Sets or overrides an item cooldown for a specific player.
    pub fn set_cooldown(&self, player_id: &str, item_id: &str, ticks: u32) {
        let mut state = self.state.write();
        let exp = state.current_tick + ticks as u64;
        state
            .cooldowns
            .insert((player_id.to_string(), item_id.to_string()), exp);
    }

    /// Executes an item action triggered by a player (e.g. right-click, left-click).
    pub fn execute_item_action(
        &self,
        player_id: &str,
        item_id: &str,
        trigger: ItemTrigger,
    ) -> ExecutionResult {
        let mut state = self.state.write();

        // 1. Check cooldown
        if let Some(exp) = find_cooldown(&state.cooldowns, player_id, item_id)
            && *exp > state.current_tick
        {
            let remaining = *exp - state.current_tick;
            return ExecutionResult {
                success: false,
                actions_executed: 0,
                events: Vec::new(),
                error_message: Some(format!(
                    "Item '{item_id}' is on cooldown for {remaining} more ticks"
                )),
            };
        }

        // 2. Find action definitions for item
        let actions = match find_matching_actions(&state.item_actions, item_id) {
            Some(a) => a.clone(),
            None => {
                return ExecutionResult {
                    success: false,
                    actions_executed: 0,
                    events: Vec::new(),
                    error_message: Some(format!("No actions registered for item '{item_id}'")),
                };
            }
        };

        // 3. Filter matching trigger
        let matching: Vec<_> = actions.into_iter().filter(|a| a.trigger == trigger).collect();
        if matching.is_empty() {
            return ExecutionResult {
                success: false,
                actions_executed: 0,
                events: Vec::new(),
                error_message: Some(format!(
                    "No actions registered for trigger '{trigger:?}' on item '{item_id}'"
                )),
            };
        }

        let mut events = Vec::new();
        let mut actions_executed = 0;

        let player = state
            .players
            .entry(player_id.to_string())
            .or_insert_with(|| PlayerState::new(player_id))
            .clone();

        let context = ExecutionContext::from_player(&player);
        let target_pos = player.position;

        for def in matching {
            // Check conditions
            let passes_conditions = def
                .conditions
                .iter()
                .all(|c| evaluate_condition(c, &context, Some(&player), None));

            if !passes_conditions {
                continue;
            }

            // Apply cooldown if specified
            if let Some(cd) = def.cooldown_ticks {
                let exp = state.current_tick + cd as u64;
                state
                    .cooldowns
                    .insert((player_id.to_string(), item_id.to_string()), exp);
            }

            // Execute each action effect
            for effect in &def.actions {
                execute_single_effect(
                    effect,
                    &mut state,
                    &context,
                    Some(player_id),
                    target_pos,
                    None,
                    &mut events,
                );
                actions_executed += 1;
            }
        }

        if actions_executed > 0 {
            ExecutionResult {
                success: true,
                actions_executed,
                events,
                error_message: None,
            }
        } else {
            ExecutionResult {
                success: false,
                actions_executed: 0,
                events: Vec::new(),
                error_message: Some("Action conditions were not met".to_string()),
            }
        }
    }

    /// Executes a block trigger at the specified coordinates (e.g. interact/click, redstone, break).
    pub fn execute_block_trigger(
        &self,
        player_id: &str,
        pos: [i32; 3],
        trigger: BlockTriggerType,
    ) -> ExecutionResult {
        let mut state = self.state.write();

        let block = match state.blocks.get(&pos).cloned() {
            Some(b) => b,
            None => {
                return ExecutionResult {
                    success: false,
                    actions_executed: 0,
                    events: Vec::new(),
                    error_message: Some(format!("No block present at coordinates {pos:?}")),
                };
            }
        };

        let triggers = match find_matching_triggers(&state.block_triggers, &block.id) {
            Some(t) => t.clone(),
            None => {
                return ExecutionResult {
                    success: false,
                    actions_executed: 0,
                    events: Vec::new(),
                    error_message: Some(format!(
                        "No triggers registered for block '{}'",
                        block.id
                    )),
                };
            }
        };

        let matching: Vec<_> = triggers.into_iter().filter(|t| t.trigger == trigger).collect();
        if matching.is_empty() {
            return ExecutionResult {
                success: false,
                actions_executed: 0,
                events: Vec::new(),
                error_message: Some(format!(
                    "No triggers registered for trigger '{trigger:?}' on block '{}'",
                    block.id
                )),
            };
        }

        let mut events = Vec::new();
        let mut actions_executed = 0;

        let player = state
            .players
            .entry(player_id.to_string())
            .or_insert_with(|| PlayerState::new(player_id))
            .clone();

        let mut context = ExecutionContext::from_player(&player);
        context.block_pos = Some(pos);
        context.block_id = Some(block.id.clone());
        context.block_properties = block.properties.clone();

        let target_pos = [
            pos[0] as f64 + 0.5,
            pos[1] as f64 + 0.5,
            pos[2] as f64 + 0.5,
        ];

        for def in matching {
            // Check conditions
            let passes_conditions = def
                .conditions
                .iter()
                .all(|c| evaluate_condition(c, &context, Some(&player), Some(&block)));

            if !passes_conditions {
                continue;
            }

            // Execute each action effect
            for effect in &def.actions {
                execute_single_effect(
                    effect,
                    &mut state,
                    &context,
                    Some(player_id),
                    target_pos,
                    Some(pos),
                    &mut events,
                );
                actions_executed += 1;
            }
        }

        if actions_executed > 0 {
            ExecutionResult {
                success: true,
                actions_executed,
                events,
                error_message: None,
            }
        } else {
            ExecutionResult {
                success: false,
                actions_executed: 0,
                events: Vec::new(),
                error_message: Some("Trigger conditions were not met".to_string()),
            }
        }
    }

    /// Directly executes an arbitrary slice of `ActionEffect`s against custom execution context.
    pub fn execute_action_effects(
        &self,
        effects: &[ActionEffect],
        context: &ExecutionContext,
    ) -> ExecutionResult {
        let mut state = self.state.write();
        let mut events = Vec::new();

        let target_pos = context.position.unwrap_or([0.0, 64.0, 0.0]);
        let player_id = context.player_id.as_deref();

        for effect in effects {
            execute_single_effect(
                effect,
                &mut state,
                context,
                player_id,
                target_pos,
                context.block_pos,
                &mut events,
            );
        }

        ExecutionResult {
            success: true,
            actions_executed: effects.len(),
            events,
            error_message: None,
        }
    }
}

fn normalize_id(mod_id: &str, id: &str) -> String {
    if id.contains(':') {
        id.to_string()
    } else {
        format!("{mod_id}:{id}")
    }
}

fn find_matching_actions<'a>(
    map: &'a HashMap<String, Vec<ItemActionDef>>,
    id: &str,
) -> Option<&'a Vec<ItemActionDef>> {
    if let Some(actions) = map.get(id) {
        return Some(actions);
    }
    if !id.contains(':') {
        let suffix = format!(":{id}");
        for (k, v) in map {
            if k.ends_with(&suffix) {
                return Some(v);
            }
        }
    }
    None
}

fn find_matching_triggers<'a>(
    map: &'a HashMap<String, Vec<BlockTriggerDef>>,
    id: &str,
) -> Option<&'a Vec<BlockTriggerDef>> {
    if let Some(triggers) = map.get(id) {
        return Some(triggers);
    }
    if !id.contains(':') {
        let suffix = format!(":{id}");
        for (k, v) in map {
            if k.ends_with(&suffix) {
                return Some(v);
            }
        }
    }
    None
}

fn find_cooldown<'a>(
    cooldowns: &'a HashMap<(String, String), u64>,
    player_id: &str,
    item_id: &str,
) -> Option<&'a u64> {
    if let Some(tick) = cooldowns.get(&(player_id.to_string(), item_id.to_string())) {
        return Some(tick);
    }
    if !item_id.contains(':') {
        let suffix = format!(":{item_id}");
        for ((p, item), tick) in cooldowns {
            if p == player_id && item.ends_with(&suffix) {
                return Some(tick);
            }
        }
    }
    None
}

fn evaluate_condition(
    condition: &ActionCondition,
    context: &ExecutionContext,
    player: Option<&PlayerState>,
    block: Option<&BlockState>,
) -> bool {
    match condition {
        ActionCondition::Sneaking { sneaking } => {
            let actual = player.map(|p| p.sneaking).unwrap_or(context.sneaking);
            actual == *sneaking
        }
        ActionCondition::HoldingItem { item_id } => {
            let held = player
                .and_then(|p| p.held_item.as_deref())
                .or(context.held_item.as_deref());
            match held {
                Some(actual) => actual == item_id || actual.ends_with(&format!(":{item_id}")),
                None => false,
            }
        }
        ActionCondition::BlockProperty { property, value } => {
            let actual_val = block
                .and_then(|b| b.properties.get(property).map(String::as_str))
                .or_else(|| context.block_properties.get(property).map(String::as_str));
            actual_val == Some(value.as_str())
        }
        ActionCondition::MinHealth { min_health } => {
            let hp = player.map(|p| p.health).unwrap_or(20.0);
            hp >= *min_health
        }
    }
}

fn execute_single_effect(
    effect: &ActionEffect,
    state: &mut MechanicsState,
    context: &ExecutionContext,
    player_id: Option<&str>,
    target_pos: [f64; 3],
    target_block_pos: Option<[i32; 3]>,
    events: &mut Vec<MechanicsEvent>,
) {
    match effect {
        ActionEffect::ApplyEffect {
            effect,
            duration_ticks,
            amplifier,
        } => {
            if let Some(pid) = player_id {
                let player = state
                    .players
                    .entry(pid.to_string())
                    .or_insert_with(|| PlayerState::new(pid));
                player.active_effects.insert(
                    effect.clone(),
                    ActiveEffect {
                        amplifier: *amplifier,
                        remaining_ticks: *duration_ticks,
                    },
                );
            }
            events.push(MechanicsEvent::EffectApplied {
                target: player_id.unwrap_or("unknown").to_string(),
                effect: effect.clone(),
                duration_ticks: *duration_ticks,
                amplifier: *amplifier,
            });
        }
        ActionEffect::Explode {
            power,
            causes_fire,
            break_blocks,
        } => {
            let mut affected_blocks = Vec::new();
            if *break_blocks {
                let radius = *power as f64;
                let radius_sq = radius * radius;
                let mut to_remove = Vec::new();
                for &pos in state.blocks.keys() {
                    let dx = pos[0] as f64 + 0.5 - target_pos[0];
                    let dy = pos[1] as f64 + 0.5 - target_pos[1];
                    let dz = pos[2] as f64 + 0.5 - target_pos[2];
                    if (dx * dx + dy * dy + dz * dz) <= radius_sq {
                        to_remove.push(pos);
                    }
                }
                for pos in to_remove {
                    state.blocks.remove(&pos);
                    affected_blocks.push(pos);
                }
            }
            events.push(MechanicsEvent::Exploded {
                pos: target_pos,
                power: *power,
                causes_fire: *causes_fire,
                break_blocks: *break_blocks,
                affected_blocks,
            });
        }
        ActionEffect::SetBlockState {
            block_id,
            properties,
        } => {
            let bpos = target_block_pos.unwrap_or([
                target_pos[0].floor() as i32,
                target_pos[1].floor() as i32,
                target_pos[2].floor() as i32,
            ]);
            let old_block = state.blocks.get(&bpos).map(|b| b.id.clone());
            let entry = state
                .blocks
                .entry(bpos)
                .or_insert_with(|| BlockState::new(block_id.as_deref().unwrap_or("minecraft:air")));

            if let Some(new_id) = block_id {
                entry.id = new_id.clone();
            }
            for (k, v) in properties {
                entry.properties.insert(k.clone(), v.clone());
            }

            events.push(MechanicsEvent::BlockStateChanged {
                pos: bpos,
                old_block,
                new_block: entry.id.clone(),
                properties: entry.properties.clone(),
            });
        }
        ActionEffect::PlaySound {
            sound,
            volume,
            pitch,
        } => {
            events.push(MechanicsEvent::SoundPlayed {
                sound: sound.clone(),
                pos: target_pos,
                volume: *volume,
                pitch: *pitch,
            });
        }
        ActionEffect::Heal { amount } => {
            if let Some(pid) = player_id {
                let player = state
                    .players
                    .entry(pid.to_string())
                    .or_insert_with(|| PlayerState::new(pid));
                player.health = (player.health + amount).min(player.max_health);
                events.push(MechanicsEvent::PlayerHealed {
                    player_id: pid.to_string(),
                    amount: *amount,
                    current_health: player.health,
                });
            }
        }
        ActionEffect::Damage {
            amount,
            damage_type,
        } => {
            if let Some(pid) = player_id {
                let player = state
                    .players
                    .entry(pid.to_string())
                    .or_insert_with(|| PlayerState::new(pid));
                player.health = (player.health - amount).max(0.0);
                events.push(MechanicsEvent::PlayerDamaged {
                    player_id: pid.to_string(),
                    amount: *amount,
                    damage_type: damage_type.clone(),
                    remaining_health: player.health,
                });
            }
        }
        ActionEffect::DamageItem { amount } => {
            if let Some(pid) = player_id {
                let item_id = state
                    .players
                    .get(pid)
                    .and_then(|p| p.held_item.clone())
                    .or_else(|| context.held_item.clone())
                    .unwrap_or_else(|| "unknown".to_string());
                events.push(MechanicsEvent::ItemDamaged {
                    player_id: pid.to_string(),
                    item_id,
                    amount: *amount,
                });
            }
        }
        ActionEffect::GiveItem { item_id, count } => {
            if let Some(pid) = player_id {
                let player = state
                    .players
                    .entry(pid.to_string())
                    .or_insert_with(|| PlayerState::new(pid));
                *player.inventory.entry(item_id.clone()).or_insert(0) += count;
                events.push(MechanicsEvent::ItemGiven {
                    player_id: pid.to_string(),
                    item_id: item_id.clone(),
                    count: *count,
                });
            }
        }
        ActionEffect::ConsumeItem { count } => {
            if let Some(pid) = player_id {
                let item_id = state
                    .players
                    .get(pid)
                    .and_then(|p| p.held_item.clone())
                    .or_else(|| context.held_item.clone())
                    .unwrap_or_else(|| "unknown".to_string());
                if let Some(player) = state.players.get_mut(pid)
                    && player.held_item_count >= *count
                {
                    player.held_item_count -= *count;
                    if player.held_item_count == 0 {
                        player.held_item = None;
                    }
                }
                events.push(MechanicsEvent::ItemConsumed {
                    player_id: pid.to_string(),
                    item_id,
                    count: *count,
                });
            }
        }
        ActionEffect::SpawnEntity { entity_type, count } => {
            events.push(MechanicsEvent::EntitySpawned {
                entity_type: entity_type.clone(),
                pos: target_pos,
                count: *count,
            });
        }
        ActionEffect::SendMessage { message } => {
            events.push(MechanicsEvent::MessageSent {
                player_id: player_id.unwrap_or("broadcast").to_string(),
                message: message.clone(),
            });
        }
        ActionEffect::Teleport {
            offset_x,
            offset_y,
            offset_z,
        } => {
            if let Some(pid) = player_id {
                let player = state
                    .players
                    .entry(pid.to_string())
                    .or_insert_with(|| PlayerState::new(pid));
                let from = player.position;
                player.position[0] += offset_x;
                player.position[1] += offset_y;
                player.position[2] += offset_z;
                let to = player.position;
                events.push(MechanicsEvent::PlayerTeleported {
                    player_id: pid.to_string(),
                    from,
                    to,
                });
            }
        }
    }
}

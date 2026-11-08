//! JVM Bytecode Action and Trigger Extractor.
//!
//! Statically analyzes Java bytecode instructions within mod item and block classes
//! to extract declarative action definitions ([`ItemActionDef`]) and block triggers
//! ([`BlockTriggerDef`]).
//!
//! Identifies entity spawning, explosions, status effects, sound playback,
//! item cooldowns, player sneaking/health conditions, and block state mutations
//! without running a JVM.

use crate::bytecode::{ClassFile, MethodInfo};
use crate::ir::{
    ActionCondition, ActionEffect, BlockTriggerDef, BlockTriggerType, ItemActionDef, ItemTrigger,
};
use crate::patterns::{decode_method_instructions, DecodedInstruction, Opcode};
use std::collections::{HashMap, HashSet, VecDeque};

/// Maximum recursion depth when following helper method invocations inside the same class.
const MAX_HELPER_CALL_DEPTH: usize = 5;

/// Maximum number of recent operands retained in rolling analysis window.
const RECENT_OPERANDS_CAPACITY: usize = 8;

/// Diagnostic or metadata information regarding detected particle emission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedParticle {
    pub particle_type: String,
    pub count: u32,
}

/// Helper rolling context tracking recently loaded constants and invocations.
#[derive(Debug, Default)]
struct MethodAnalysisState {
    recent_ints: VecDeque<i32>,
    recent_floats: VecDeque<f32>,
    recent_strings: VecDeque<String>,
    recent_classes: VecDeque<String>,
    recent_fields: VecDeque<(String, String)>,
    actions: Vec<ActionEffect>,
    conditions: Vec<ActionCondition>,
    cooldown_ticks: Option<u32>,
    particles: Vec<DetectedParticle>,
}

impl MethodAnalysisState {
    fn new() -> Self {
        Self::default()
    }

    fn push_int(&mut self, val: i32) {
        if self.recent_ints.len() >= RECENT_OPERANDS_CAPACITY {
            self.recent_ints.pop_front();
        }
        self.recent_ints.push_back(val);
    }

    fn push_float(&mut self, val: f32) {
        if self.recent_floats.len() >= RECENT_OPERANDS_CAPACITY {
            self.recent_floats.pop_front();
        }
        self.recent_floats.push_back(val);
    }

    fn push_string(&mut self, s: String) {
        if self.recent_strings.len() >= RECENT_OPERANDS_CAPACITY {
            self.recent_strings.pop_front();
        }
        self.recent_strings.push_back(s);
    }

    fn push_class(&mut self, c: String) {
        if self.recent_classes.len() >= RECENT_OPERANDS_CAPACITY {
            self.recent_classes.pop_front();
        }
        self.recent_classes.push_back(c);
    }

    fn push_field(&mut self, class_name: String, field_name: String) {
        if self.recent_fields.len() >= RECENT_OPERANDS_CAPACITY {
            self.recent_fields.pop_front();
        }
        self.recent_fields.push_back((class_name, field_name));
    }

    fn add_action(&mut self, action: ActionEffect) {
        if !self.actions.contains(&action) {
            self.actions.push(action);
        }
    }

    fn add_condition(&mut self, condition: ActionCondition) {
        if !self.conditions.contains(&condition) {
            self.conditions.push(condition);
        }
    }
}

/// Maps an item method name to its corresponding declarative [`ItemTrigger`].
pub fn map_item_method_to_trigger(name: &str) -> Option<ItemTrigger> {
    match name {
        "use" | "onItemRightClick" | "interactLivingEntity" | "itemInteractionForEntity" => {
            Some(ItemTrigger::RightClick)
        }
        "useOn" | "onItemUse" | "onItemUseFirst" => Some(ItemTrigger::RightClickBlock),
        "finishUsingItem" | "onItemUseFinish" => Some(ItemTrigger::Consume),
        "hurt" | "onHit" | "hurtEnemy" | "hitEntity" => Some(ItemTrigger::HitEntity),
        "onLeftClickEntity" | "attack" => Some(ItemTrigger::LeftClick),
        _ => None,
    }
}

/// Maps a block method name to its corresponding declarative [`BlockTriggerType`].
pub fn map_block_method_to_trigger(name: &str) -> Option<BlockTriggerType> {
    match name {
        "use" | "useWithoutItem" | "onBlockActivated" | "interact" => {
            Some(BlockTriggerType::Interact)
        }
        "onRemove" | "destroy" | "breakBlock" | "onBlockDestroyedByPlayer"
        | "playerWillDestroy" => Some(BlockTriggerType::Break),
        "stepOn" | "onSteppedOn" | "onEntityCollision" | "entityInside" => {
            Some(BlockTriggerType::StepOn)
        }
        "neighborChanged" | "neighborUpdate" => Some(BlockTriggerType::NeighborUpdate),
        "randomTick" | "tick" | "animateTick" => Some(BlockTriggerType::RandomTick),
        "getSignal" | "getDirectSignal" | "onPlace" => Some(BlockTriggerType::Redstone),
        _ => None,
    }
}

/// Normalizes a status effect field or identifier to its canonical Minecraft ID.
pub fn normalize_mob_effect_name(name: &str) -> String {
    let lower = name.to_lowercase();
    match lower.as_str() {
        "movement_speed" | "speed" => "minecraft:speed".to_string(),
        "movement_slowdown" | "slowness" => "minecraft:slowness".to_string(),
        "dig_speed" | "haste" => "minecraft:haste".to_string(),
        "dig_slowdown" | "mining_fatigue" => "minecraft:mining_fatigue".to_string(),
        "damage_boost" | "strength" => "minecraft:strength".to_string(),
        "heal" | "instant_health" => "minecraft:instant_health".to_string(),
        "harm" | "instant_damage" => "minecraft:instant_damage".to_string(),
        "jump" | "jump_boost" => "minecraft:jump_boost".to_string(),
        "confusion" | "nausea" => "minecraft:nausea".to_string(),
        "regeneration" => "minecraft:regeneration".to_string(),
        "damage_resistance" | "resistance" => "minecraft:resistance".to_string(),
        "fire_resistance" => "minecraft:fire_resistance".to_string(),
        "water_breathing" => "minecraft:water_breathing".to_string(),
        "invisibility" => "minecraft:invisibility".to_string(),
        "blindness" => "minecraft:blindness".to_string(),
        "night_vision" => "minecraft:night_vision".to_string(),
        "hunger" => "minecraft:hunger".to_string(),
        "weakness" => "minecraft:weakness".to_string(),
        "poison" => "minecraft:poison".to_string(),
        "wither" => "minecraft:wither".to_string(),
        "health_boost" => "minecraft:health_boost".to_string(),
        "absorption" => "minecraft:absorption".to_string(),
        "saturation" => "minecraft:saturation".to_string(),
        "glowing" => "minecraft:glowing".to_string(),
        "levitation" => "minecraft:levitation".to_string(),
        "luck" => "minecraft:luck".to_string(),
        "unluck" | "bad_luck" => "minecraft:bad_luck".to_string(),
        "slow_falling" => "minecraft:slow_falling".to_string(),
        "conduit_power" => "minecraft:conduit_power".to_string(),
        "dolphins_grace" => "minecraft:dolphins_grace".to_string(),
        "bad_omen" => "minecraft:bad_omen".to_string(),
        "hero_of_the_village" => "minecraft:hero_of_the_village".to_string(),
        "darkness" => "minecraft:darkness".to_string(),
        other => {
            if other.contains(':') {
                other.to_string()
            } else {
                format!("minecraft:{other}")
            }
        }
    }
}

/// Normalizes a sound event field or name to its canonical Minecraft sound identifier.
pub fn normalize_sound_name(name: &str) -> String {
    let lower = name.to_lowercase();
    match lower.as_str() {
        "arrow_shoot" | "entity_arrow_shoot" => "minecraft:entity.arrow.shoot".to_string(),
        "generic_explode" | "entity_generic_explode" => {
            "minecraft:entity.generic.explode".to_string()
        }
        "generic_eat" | "entity_generic_eat" => "minecraft:entity.generic.eat".to_string(),
        "generic_drink" | "entity_generic_drink" => "minecraft:entity.generic.drink".to_string(),
        "generic_burn" | "entity_generic_burn" => "minecraft:entity.generic.burn".to_string(),
        "chest_open" | "block_chest_open" => "minecraft:block.chest.open".to_string(),
        "chest_close" | "block_chest_close" => "minecraft:block.chest.close".to_string(),
        "anvil_use" | "block_anvil_use" => "minecraft:block.anvil.use".to_string(),
        "player_levelup" | "entity_player_levelup" | "levelup" => {
            "minecraft:entity.player.levelup".to_string()
        }
        "firecharge_use" | "item_firecharge_use" => "minecraft:item.firecharge.use".to_string(),
        "flintandsteel_use" | "item_flintandsteel_use" => {
            "minecraft:item.flintandsteel.use".to_string()
        }
        other => {
            if other.contains(':') {
                other.to_string()
            } else {
                let dotted = other.replace('_', ".");
                format!("minecraft:{dotted}")
            }
        }
    }
}

/// Normalizes a projectile or spawned entity class name to a canonical Minecraft or mod entity identifier.
pub fn normalize_projectile_entity_type(class_name: &str, mod_namespace: &str) -> String {
    let simple_name = class_name
        .rsplit('/')
        .next()
        .unwrap_or(class_name)
        .rsplit('.')
        .next()
        .unwrap_or(class_name);

    match simple_name {
        "Arrow" => "minecraft:arrow".to_string(),
        "SpectralArrow" => "minecraft:spectral_arrow".to_string(),
        "SmallFireball" => "minecraft:small_fireball".to_string(),
        "LargeFireball" | "Fireball" => "minecraft:fireball".to_string(),
        "DragonFireball" => "minecraft:dragon_fireball".to_string(),
        "Snowball" => "minecraft:snowball".to_string(),
        "ThrownEgg" | "Egg" => "minecraft:egg".to_string(),
        "ThrownEnderpearl" | "Enderpearl" => "minecraft:ender_pearl".to_string(),
        "ThrownExperienceBottle" | "ExperienceBottle" => "minecraft:experience_bottle".to_string(),
        "ThrownPotion" | "Potion" => "minecraft:potion".to_string(),
        "ThrownTrident" | "Trident" => "minecraft:trident".to_string(),
        "WitherSkull" => "minecraft:wither_skull".to_string(),
        "ShulkerBullet" => "minecraft:shulker_bullet".to_string(),
        "LlamaSpit" => "minecraft:llama_spit".to_string(),
        "EyeOfEnder" => "minecraft:eye_of_ender".to_string(),
        "PrimedTnt" | "Tnt" => "minecraft:tnt".to_string(),
        "LightningBolt" => "minecraft:lightning_bolt".to_string(),
        custom => {
            let trimmed = custom.strip_suffix("Entity").unwrap_or(custom);
            let mut snake = String::new();
            for (i, ch) in trimmed.chars().enumerate() {
                if ch.is_uppercase() {
                    if i > 0 {
                        snake.push('_');
                    }
                    for lc in ch.to_lowercase() {
                        snake.push(lc);
                    }
                } else {
                    snake.push(ch);
                }
            }
            if mod_namespace.is_empty() {
                format!("minecraft:{snake}")
            } else {
                format!("{mod_namespace}:{snake}")
            }
        }
    }
}

/// Recursively traverses bytecode instructions of `method` (and internal helper calls)
/// to detect effects, conditions, and cooldown constraints.
fn analyze_method_instructions(
    method: &MethodInfo,
    class: &ClassFile,
    mod_namespace: &str,
    visited: &mut HashSet<String>,
    depth: usize,
    state: &mut MethodAnalysisState,
) {
    if depth > MAX_HELPER_CALL_DEPTH {
        return;
    }

    let code_info = match &method.code {
        Some(c) => c,
        None => return,
    };

    let instructions = decode_method_instructions(&code_info.bytecode, &class.constant_pool);

    for (idx, inst) in instructions.iter().enumerate() {
        match &inst.op {
            Opcode::ConstInt(val) => state.push_int(*val),
            Opcode::LdcInt(val) => state.push_int(*val),
            Opcode::ConstFloat(val) => state.push_float(*val),
            Opcode::LdcFloat(val) => state.push_float(*val),
            Opcode::LdcString(s) => state.push_string(s.clone()),
            Opcode::LdcClass(c) => state.push_class(c.clone()),
            Opcode::New(cls) => {
                state.push_class(cls.clone());
                // Instantiation of Projectile classes can indicate entity spawn even without direct addFreshEntity
                if cls.contains("Projectile")
                    || cls.ends_with("/Arrow")
                    || cls.ends_with("/SmallFireball")
                    || cls.ends_with("/Snowball")
                    || cls.ends_with("/Fireball")
                {
                    let entity_type = normalize_projectile_entity_type(cls, mod_namespace);
                    state.add_action(ActionEffect::SpawnEntity {
                        entity_type,
                        count: 1,
                    });
                }
            }
            Opcode::GetStatic {
                class_name,
                name,
                descriptor: _,
            } => {
                state.push_field(class_name.clone(), name.clone());
            }
            Opcode::InvokeVirtual {
                class_name,
                name,
                descriptor,
            }
            | Opcode::InvokeSpecial {
                class_name,
                name,
                descriptor,
            }
            | Opcode::InvokeStatic {
                class_name,
                name,
                descriptor,
            }
            | Opcode::InvokeInterface {
                class_name,
                name,
                descriptor,
            } => {
                handle_method_invocation(
                    class_name,
                    name,
                    descriptor,
                    inst.raw_opcode,
                    idx,
                    &instructions,
                    class,
                    mod_namespace,
                    visited,
                    depth,
                    state,
                );
            }
            Opcode::Other(0x99 | 0x9A) => {
                // ifeq / ifne branch instructions
            }
            Opcode::Other(_) => {}
            _ => {}
        }
    }
}

/// Dispatches actions and conditions based on target method invocation.
#[allow(clippy::too_many_arguments)]
fn handle_method_invocation(
    class_name: &str,
    name: &str,
    _descriptor: &str,
    _raw_opcode: u8,
    inst_idx: usize,
    instructions: &[DecodedInstruction],
    class: &ClassFile,
    mod_namespace: &str,
    visited: &mut HashSet<String>,
    depth: usize,
    state: &mut MethodAnalysisState,
) {
    let lower_name = name.to_lowercase();
    let lower_class = class_name.to_lowercase();

    // 1. Entity Spawning & Explosions
    if lower_name == "addfreshentity" || lower_name == "spawnentity" || lower_name == "spawn" {
        // Look up target entity class from recent instantiations or strings
        let entity_type = state
            .recent_classes
            .iter()
            .rev()
            .find(|c| {
                c.contains("Entity")
                    || c.contains("Projectile")
                    || c.contains("Arrow")
                    || c.contains("Fireball")
                    || c.contains("Snowball")
            })
            .map(|c| normalize_projectile_entity_type(c, mod_namespace))
            .or_else(|| {
                state
                    .recent_strings
                    .iter()
                    .rev()
                    .find(|s| s.contains(':'))
                    .cloned()
            })
            .unwrap_or_else(|| "minecraft:arrow".to_string());

        state.add_action(ActionEffect::SpawnEntity {
            entity_type,
            count: 1,
        });
    } else if lower_name == "explode" || lower_name == "createexplosion" {
        let power = state
            .recent_floats
            .iter()
            .rev()
            .find(|&&f| f > 0.0)
            .copied()
            .unwrap_or(3.0);

        state.add_action(ActionEffect::Explode {
            power,
            causes_fire: false,
            break_blocks: true,
        });
    }
    // 2. Status Effect Application
    else if lower_name == "addeffect"
        || lower_name == "addstatuseffect"
        || lower_name == "haseffect"
        || (lower_name == "<init>"
            && (lower_class.ends_with("mobeffectinstance")
                || lower_class.ends_with("statuseffectinstance")
                || lower_class.ends_with("potioneffect")))
    {
        let effect = state
            .recent_fields
            .iter()
            .rev()
            .find(|(cls, _)| {
                cls.ends_with("MobEffects")
                    || cls.ends_with("StatusEffects")
                    || cls.ends_with("PotionEffects")
                    || cls.ends_with("Effects")
            })
            .map(|(_, fld)| normalize_mob_effect_name(fld))
            .or_else(|| {
                state
                    .recent_strings
                    .iter()
                    .rev()
                    .find(|s| !s.is_empty())
                    .cloned()
            })
            .unwrap_or_else(|| "minecraft:speed".to_string());

        let duration_ticks = state
            .recent_ints
            .iter()
            .rev()
            .find(|&&i| i >= 10)
            .copied()
            .map(|i| i as u32)
            .unwrap_or(200);

        let amplifier = state
            .recent_ints
            .iter()
            .rev()
            .find(|&&i| (0..=10).contains(&i))
            .copied()
            .map(|i| i as u32)
            .unwrap_or(0);

        state.add_action(ActionEffect::ApplyEffect {
            effect,
            duration_ticks,
            amplifier,
        });
    }
    // 3. Sound Playback
    else if lower_name == "playsound" || lower_name == "playlocalsound" {
        let sound = state
            .recent_fields
            .iter()
            .rev()
            .find(|(cls, _)| {
                cls.contains("SoundEvents") || cls.contains("Sounds") || cls.contains("SoundEvent")
            })
            .map(|(_, fld)| normalize_sound_name(fld))
            .or_else(|| {
                state
                    .recent_strings
                    .iter()
                    .rev()
                    .find(|s| s.contains('.') || s.contains(':'))
                    .cloned()
            })
            .unwrap_or_else(|| "minecraft:entity.arrow.shoot".to_string());

        let volume = state
            .recent_floats
            .iter()
            .rev()
            .find(|&&f| f > 0.0)
            .copied()
            .unwrap_or(1.0);

        let pitch = state
            .recent_floats
            .iter()
            .rev()
            .filter(|&&f| f > 0.0)
            .nth(1)
            .copied()
            .unwrap_or(1.0);

        state.add_action(ActionEffect::PlaySound {
            sound,
            volume,
            pitch,
        });
    }
    // 4. Particle Emission Detection
    else if lower_name == "sendparticles"
        || lower_name == "addparticle"
        || lower_name == "spawnparticle"
    {
        let particle_type = state
            .recent_fields
            .iter()
            .rev()
            .find(|(cls, _)| cls.contains("ParticleTypes") || cls.contains("Particles"))
            .map(|(_, fld)| fld.to_lowercase())
            .or_else(|| {
                state
                    .recent_strings
                    .iter()
                    .rev()
                    .find(|s| !s.is_empty())
                    .cloned()
            })
            .unwrap_or_else(|| "flame".to_string());

        let count = state
            .recent_ints
            .iter()
            .rev()
            .find(|&&i| i > 0 && i <= 1000)
            .copied()
            .map(|i| i as u32)
            .unwrap_or(1);

        state.particles.push(DetectedParticle {
            particle_type,
            count,
        });
    }
    // 5. Cooldown Configuration
    else if lower_name == "addcooldown" || (lower_name == "set" && lower_class.contains("cooldown"))
    {
        if let Some(&ticks) = state.recent_ints.iter().rev().find(|&&i| i > 0) {
            state.cooldown_ticks = Some(ticks as u32);
        }
    }
    // 6. Condition Checks: Sneaking, Health, Held Item, Block Property
    else if lower_name == "isshiftkeydown"
        || lower_name == "issneaking"
        || lower_name == "iscrouching"
    {
        // Check next branch instruction if available
        let sneaking = if inst_idx + 1 < instructions.len() {
            // ifeq (0x99) branches away when false, meaning block continues if sneaking == true
            instructions[inst_idx + 1].raw_opcode != 0x9A
        } else {
            true
        };
        state.add_condition(ActionCondition::Sneaking { sneaking });
    } else if lower_name == "gethealth" {
        let min_health = state
            .recent_floats
            .iter()
            .rev()
            .find(|&&f| f > 0.0)
            .copied()
            .unwrap_or(5.0);
        state.add_condition(ActionCondition::MinHealth { min_health });
    } else if lower_name == "isholding"
        || lower_name == "getmainhanditem"
        || lower_name == "getoffhanditem"
    {
        if let Some(item) = state
            .recent_strings
            .iter()
            .rev()
            .find(|s| s.contains(':') || !s.is_empty())
        {
            state.add_condition(ActionCondition::HoldingItem {
                item_id: item.clone(),
            });
        }
    } else if lower_name == "getvalue" || lower_name == "hasproperty" {
        if state.recent_strings.len() >= 2 {
            let mut iter = state.recent_strings.iter().rev();
            let val = iter.next().unwrap().clone();
            let prop = iter.next().unwrap().clone();
            state.add_condition(ActionCondition::BlockProperty {
                property: prop,
                value: val,
            });
        }
    }
    // 7. Block State Mutation
    else if lower_name == "setblock" || lower_name == "setblockstate" || lower_name == "cycle" {
        state.add_action(ActionEffect::SetBlockState {
            block_id: None,
            properties: HashMap::new(),
        });
    }
    // 8. Health, Item Durability, and Inventory Manipulation
    else if lower_name == "heal" {
        let amount = state
            .recent_floats
            .iter()
            .rev()
            .find(|&&f| f > 0.0)
            .copied()
            .unwrap_or(4.0);
        state.add_action(ActionEffect::Heal { amount });
    } else if lower_name == "hurt" || lower_name == "damage" {
        let amount = state
            .recent_floats
            .iter()
            .rev()
            .find(|&&f| f > 0.0)
            .copied()
            .unwrap_or(4.0);
        state.add_action(ActionEffect::Damage {
            amount,
            damage_type: None,
        });
    } else if lower_name == "hurtandbreak" || lower_name == "damageitem" {
        let amount = state
            .recent_ints
            .iter()
            .rev()
            .find(|&&i| i > 0)
            .copied()
            .map(|i| i as u32)
            .unwrap_or(1);
        state.add_action(ActionEffect::DamageItem { amount });
    } else if lower_name == "giveitem" || lower_name == "additem" {
        let item_id = state
            .recent_strings
            .iter()
            .rev()
            .find(|s| s.contains(':'))
            .cloned()
            .unwrap_or_else(|| "minecraft:iron_ingot".to_string());
        state.add_action(ActionEffect::GiveItem { item_id, count: 1 });
    } else if lower_name == "shrink" || lower_name == "consume" {
        state.add_action(ActionEffect::ConsumeItem { count: 1 });
    } else if lower_name == "teleportto"
        || lower_name == "randomteleport"
        || lower_name == "teleport"
    {
        state.add_action(ActionEffect::Teleport {
            offset_x: 0.0,
            offset_y: 0.0,
            offset_z: 0.0,
        });
    } else if lower_name == "sendmessage" || lower_name == "displayclientmessage" {
        let message = state
            .recent_strings
            .iter()
            .rev()
            .find(|s| !s.is_empty())
            .cloned()
            .unwrap_or_else(|| "Action triggered".to_string());
        state.add_action(ActionEffect::SendMessage { message });
    }

    // 9. Follow helper methods declared in the same class
    let this_class_name = class.class_name();
    let can_descend = (class_name == this_class_name || class_name.is_empty())
        && !visited.contains(name)
        && depth < MAX_HELPER_CALL_DEPTH;
    if !can_descend {
        return;
    }

    if let Some(target_method) = class.find_method(name) {
        visited.insert(name.to_string());
        analyze_method_instructions(
            target_method,
            class,
            mod_namespace,
            visited,
            depth + 1,
            state,
        );
    }
}

/// Analyzes a single candidate item method and constructs an [`ItemActionDef`] if actions are found.
fn extract_action_from_method(
    method: &MethodInfo,
    class: &ClassFile,
    item_id: &str,
) -> Option<ItemActionDef> {
    let trigger = map_item_method_to_trigger(&method.name)?;
    let mod_namespace = item_id.split(':').next().unwrap_or("minecraft");

    let mut state = MethodAnalysisState::new();
    let mut visited = HashSet::new();
    visited.insert(method.name.clone());

    analyze_method_instructions(method, class, mod_namespace, &mut visited, 0, &mut state);

    if state.actions.is_empty() && state.cooldown_ticks.is_none() {
        return None;
    }

    let mut def = ItemActionDef::new(trigger, state.actions).with_item_id(item_id);

    if let Some(ticks) = state.cooldown_ticks {
        def = def.with_cooldown(ticks);
    }

    for cond in state.conditions {
        def = def.with_condition(cond);
    }

    Some(def)
}

/// Extracts all item action definitions from the provided slice of methods.
pub fn extract_item_actions_from_methods(
    methods: &[MethodInfo],
    class: &ClassFile,
    item_id: &str,
) -> Vec<ItemActionDef> {
    let mut actions = Vec::new();

    for method in methods {
        if let Some(action) = extract_action_from_method(method, class, item_id) {
            actions.push(action);
        }
    }

    actions
}

/// Primary item action extraction entry point accepting methods slice, class, and item identifier.
///
/// Returns the first extracted [`ItemActionDef`], prioritizing right-click usage if present.
pub fn extract_item_action(
    methods: &[MethodInfo],
    class: &ClassFile,
    item_id: &str,
) -> Option<ItemActionDef> {
    let mut actions = extract_item_actions_from_methods(methods, class, item_id);

    // Prioritize right-click triggers
    if let Some(pos) = actions
        .iter()
        .position(|a| a.trigger == ItemTrigger::RightClick)
    {
        return Some(actions.remove(pos));
    }

    actions.into_iter().next()
}

/// Extracts all item action definitions declared in the class file.
pub fn extract_item_actions(class: &ClassFile, item_id: &str) -> Vec<ItemActionDef> {
    extract_item_actions_from_methods(class.methods(), class, item_id)
}

/// Extracts all block trigger definitions declared in the class file.
pub fn extract_block_triggers(class: &ClassFile, block_id: &str) -> Vec<BlockTriggerDef> {
    extract_block_triggers_from_methods(class.methods(), class, block_id)
}

/// Extracts all block trigger definitions from the provided slice of methods.
pub fn extract_block_triggers_from_methods(
    methods: &[MethodInfo],
    class: &ClassFile,
    block_id: &str,
) -> Vec<BlockTriggerDef> {
    let mod_namespace = block_id.split(':').next().unwrap_or("minecraft");
    let mut triggers = Vec::new();

    for method in methods {
        if let Some(trigger_type) = map_block_method_to_trigger(&method.name) {
            let mut state = MethodAnalysisState::new();
            let mut visited = HashSet::new();
            visited.insert(method.name.clone());

            analyze_method_instructions(
                method,
                class,
                mod_namespace,
                &mut visited,
                0,
                &mut state,
            );

            if !state.actions.is_empty() {
                let mut def =
                    BlockTriggerDef::new(trigger_type, state.actions).with_block_id(block_id);
                for cond in state.conditions {
                    def = def.with_condition(cond);
                }
                triggers.push(def);
            }
        }
    }

    triggers
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bytecode::{CodeInfo, ConstantPool, ConstantPoolEntry, MethodInfo};

    fn create_test_class(
        _class_name: &str,
        methods: Vec<MethodInfo>,
        constant_pool: ConstantPool,
    ) -> ClassFile {
        ClassFile {
            minor_version: 0,
            major_version: 65,
            constant_pool,
            access_flags: 0x0001,
            this_class: 1,
            super_class: 0,
            interfaces: vec![],
            fields: vec![],
            methods,
            attributes: vec![],
        }
    }

    #[test]
    fn test_extract_spawn_projectile_and_cooldown() {
        let mut cp = ConstantPool::new();
        // 1: this class
        cp.push(ConstantPoolEntry::Class { name_index: 2 });
        // 2: "com/example/FireStaff"
        cp.push(ConstantPoolEntry::Utf8("com/example/FireStaff".into()));
        // 3: Projectile class
        cp.push(ConstantPoolEntry::Class { name_index: 4 });
        // 4: "net/minecraft/world/entity/projectile/SmallFireball"
        cp.push(ConstantPoolEntry::Utf8(
            "net/minecraft/world/entity/projectile/SmallFireball".into(),
        ));
        // 5: Methodref Level.addFreshEntity
        cp.push(ConstantPoolEntry::Methodref {
            class_index: 6,
            name_and_type_index: 7,
        });
        // 6: Level class
        cp.push(ConstantPoolEntry::Class { name_index: 8 });
        // 7: NameAndType addFreshEntity
        cp.push(ConstantPoolEntry::NameAndType {
            name_index: 9,
            descriptor_index: 10,
        });
        // 8: "net/minecraft/world/level/Level"
        cp.push(ConstantPoolEntry::Utf8(
            "net/minecraft/world/level/Level".into(),
        ));
        // 9: "addFreshEntity"
        cp.push(ConstantPoolEntry::Utf8("addFreshEntity".into()));
        // 10: "(Lnet/minecraft/world/entity/Entity;)Z"
        cp.push(ConstantPoolEntry::Utf8(
            "(Lnet/minecraft/world/entity/Entity;)Z".into(),
        ));
        // 11: Methodref ItemCooldowns.addCooldown
        cp.push(ConstantPoolEntry::Methodref {
            class_index: 12,
            name_and_type_index: 13,
        });
        // 12: ItemCooldowns class
        cp.push(ConstantPoolEntry::Class { name_index: 14 });
        // 13: NameAndType addCooldown
        cp.push(ConstantPoolEntry::NameAndType {
            name_index: 15,
            descriptor_index: 16,
        });
        // 14: "net/minecraft/world/item/ItemCooldowns"
        cp.push(ConstantPoolEntry::Utf8(
            "net/minecraft/world/item/ItemCooldowns".into(),
        ));
        // 15: "addCooldown"
        cp.push(ConstantPoolEntry::Utf8("addCooldown".into()));
        // 16: "(Lnet/minecraft/world/item/Item;I)V"
        cp.push(ConstantPoolEntry::Utf8(
            "(Lnet/minecraft/world/item/Item;I)V".into(),
        ));

        // Synthetic bytecode for method "use":
        // 1. new SmallFireball (0xBB, index 3 -> [0x00, 0x03])
        // 2. invokevirtual Level.addFreshEntity (0xB6, index 5 -> [0x00, 0x05])
        // 3. bipush 30 (0x10, 30)
        // 4. invokevirtual ItemCooldowns.addCooldown (0xB6, index 11 -> [0x00, 0x0B])
        // 5. return (0xB1)
        let bytecode = vec![
            0xBB, 0x00, 0x03, // new SmallFireball
            0xB6, 0x00, 0x05, // invokevirtual addFreshEntity
            0x10, 30, // bipush 30
            0xB6, 0x00, 0x0B, // invokevirtual addCooldown
            0xB1, // return
        ];

        let method = MethodInfo {
            access_flags: 0x0001,
            name_index: 0,
            name: "use".into(),
            descriptor_index: 0,
            descriptor: "()V".into(),
            code: Some(CodeInfo {
                max_stack: 4,
                max_locals: 4,
                bytecode,
            }),
            attributes: vec![],
        };

        let class = create_test_class("com/example/FireStaff", vec![method], cp);
        let action = extract_item_action(class.methods(), &class, "example:fire_staff");

        assert!(action.is_some(), "Expected item action to be extracted");
        let def = action.unwrap();
        assert_eq!(def.trigger, ItemTrigger::RightClick);
        assert_eq!(def.cooldown_ticks, Some(30));
        assert!(def.actions.contains(&ActionEffect::SpawnEntity {
            entity_type: "minecraft:small_fireball".into(),
            count: 1,
        }));
    }

    #[test]
    fn test_extract_explosion_and_sound() {
        let mut cp = ConstantPool::new();
        // 1: this class
        cp.push(ConstantPoolEntry::Class { name_index: 2 });
        // 2: "com/example/BoomStick"
        cp.push(ConstantPoolEntry::Utf8("com/example/BoomStick".into()));
        // 3: Methodref Level.explode
        cp.push(ConstantPoolEntry::Methodref {
            class_index: 4,
            name_and_type_index: 5,
        });
        // 4: Level class
        cp.push(ConstantPoolEntry::Class { name_index: 6 });
        // 5: NameAndType explode
        cp.push(ConstantPoolEntry::NameAndType {
            name_index: 7,
            descriptor_index: 8,
        });
        // 6: "net/minecraft/world/level/Level"
        cp.push(ConstantPoolEntry::Utf8(
            "net/minecraft/world/level/Level".into(),
        ));
        // 7: "explode"
        cp.push(ConstantPoolEntry::Utf8("explode".into()));
        // 8: "()V"
        cp.push(ConstantPoolEntry::Utf8("()V".into()));
        // 9: Methodref Level.playSound
        cp.push(ConstantPoolEntry::Methodref {
            class_index: 4,
            name_and_type_index: 10,
        });
        // 10: NameAndType playSound
        cp.push(ConstantPoolEntry::NameAndType {
            name_index: 11,
            descriptor_index: 8,
        });
        // 11: "playSound"
        cp.push(ConstantPoolEntry::Utf8("playSound".into()));
        // 12: Fieldref SoundEvents.GENERIC_EXPLODE
        cp.push(ConstantPoolEntry::Fieldref {
            class_index: 13,
            name_and_type_index: 14,
        });
        // 13: SoundEvents class
        cp.push(ConstantPoolEntry::Class { name_index: 15 });
        // 14: NameAndType GENERIC_EXPLODE
        cp.push(ConstantPoolEntry::NameAndType {
            name_index: 16,
            descriptor_index: 17,
        });
        // 15: "net/minecraft/sounds/SoundEvents"
        cp.push(ConstantPoolEntry::Utf8(
            "net/minecraft/sounds/SoundEvents".into(),
        ));
        // 16: "GENERIC_EXPLODE"
        cp.push(ConstantPoolEntry::Utf8("GENERIC_EXPLODE".into()));
        // 17: "Lnet/minecraft/sounds/SoundEvent;"
        cp.push(ConstantPoolEntry::Utf8(
            "Lnet/minecraft/sounds/SoundEvent;".into(),
        ));

        // Synthetic bytecode for method "useOn" (RightClickBlock):
        // 1. fconst_2 (0x0D) -> 2.0
        // 2. invokevirtual Level.explode (0xB6, index 3 -> [0x00, 0x03])
        // 3. getstatic SoundEvents.GENERIC_EXPLODE (0xB2, index 12 -> [0x00, 0x0C])
        // 4. fconst_1 (0x0C) -> volume 1.0
        // 5. fconst_1 (0x0C) -> pitch 1.0
        // 6. invokevirtual Level.playSound (0xB6, index 9 -> [0x00, 0x09])
        // 7. return (0xB1)
        let bytecode = vec![
            0x0D, // fconst_2
            0xB6, 0x00, 0x03, // invokevirtual explode
            0xB2, 0x00, 0x0C, // getstatic GENERIC_EXPLODE
            0x0C, // fconst_1
            0x0C, // fconst_1
            0xB6, 0x00, 0x09, // invokevirtual playSound
            0xB1, // return
        ];

        let method = MethodInfo {
            access_flags: 0x0001,
            name_index: 0,
            name: "useOn".into(),
            descriptor_index: 0,
            descriptor: "()V".into(),
            code: Some(CodeInfo {
                max_stack: 4,
                max_locals: 4,
                bytecode,
            }),
            attributes: vec![],
        };

        let class = create_test_class("com/example/BoomStick", vec![method], cp);
        let action = extract_item_action(class.methods(), &class, "example:boom_stick");

        assert!(action.is_some());
        let def = action.unwrap();
        assert_eq!(def.trigger, ItemTrigger::RightClickBlock);
        assert!(def.actions.contains(&ActionEffect::Explode {
            power: 2.0,
            causes_fire: false,
            break_blocks: true,
        }));
        assert!(def.actions.contains(&ActionEffect::PlaySound {
            sound: "minecraft:entity.generic.explode".into(),
            volume: 1.0,
            pitch: 1.0,
        }));
    }

    #[test]
    fn test_extract_apply_effect_and_conditions() {
        let mut cp = ConstantPool::new();
        // 1: this class
        cp.push(ConstantPoolEntry::Class { name_index: 2 });
        // 2: "com/example/PoisonApple"
        cp.push(ConstantPoolEntry::Utf8("com/example/PoisonApple".into()));
        // 3: Fieldref MobEffects.POISON
        cp.push(ConstantPoolEntry::Fieldref {
            class_index: 4,
            name_and_type_index: 5,
        });
        // 4: MobEffects class
        cp.push(ConstantPoolEntry::Class { name_index: 6 });
        // 5: NameAndType POISON
        cp.push(ConstantPoolEntry::NameAndType {
            name_index: 7,
            descriptor_index: 8,
        });
        // 6: "net/minecraft/world/effect/MobEffects"
        cp.push(ConstantPoolEntry::Utf8(
            "net/minecraft/world/effect/MobEffects".into(),
        ));
        // 7: "POISON"
        cp.push(ConstantPoolEntry::Utf8("POISON".into()));
        // 8: "Lnet/minecraft/world/effect/MobEffect;"
        cp.push(ConstantPoolEntry::Utf8(
            "Lnet/minecraft/world/effect/MobEffect;".into(),
        ));
        // 9: Methodref LivingEntity.addEffect
        cp.push(ConstantPoolEntry::Methodref {
            class_index: 10,
            name_and_type_index: 11,
        });
        // 10: LivingEntity class
        cp.push(ConstantPoolEntry::Class { name_index: 12 });
        // 11: NameAndType addEffect
        cp.push(ConstantPoolEntry::NameAndType {
            name_index: 13,
            descriptor_index: 14,
        });
        // 12: "net/minecraft/world/entity/LivingEntity"
        cp.push(ConstantPoolEntry::Utf8(
            "net/minecraft/world/entity/LivingEntity".into(),
        ));
        // 13: "addEffect"
        cp.push(ConstantPoolEntry::Utf8("addEffect".into()));
        // 14: "(Lnet/minecraft/world/effect/MobEffectInstance;)Z"
        cp.push(ConstantPoolEntry::Utf8(
            "(Lnet/minecraft/world/effect/MobEffectInstance;)Z".into(),
        ));
        // 15: Methodref Player.isShiftKeyDown
        cp.push(ConstantPoolEntry::Methodref {
            class_index: 16,
            name_and_type_index: 17,
        });
        // 16: Player class
        cp.push(ConstantPoolEntry::Class { name_index: 18 });
        // 17: NameAndType isShiftKeyDown
        cp.push(ConstantPoolEntry::NameAndType {
            name_index: 19,
            descriptor_index: 20,
        });
        // 18: "net/minecraft/world/entity/player/Player"
        cp.push(ConstantPoolEntry::Utf8(
            "net/minecraft/world/entity/player/Player".into(),
        ));
        // 19: "isShiftKeyDown"
        cp.push(ConstantPoolEntry::Utf8("isShiftKeyDown".into()));
        // 20: "()Z"
        cp.push(ConstantPoolEntry::Utf8("()Z".into()));

        // Synthetic bytecode for method "finishUsingItem" (Consume):
        // 1. invokevirtual Player.isShiftKeyDown (0xB6, index 15 -> [0x00, 0x0F])
        // 2. ifeq +5 (0x99, [0x00, 0x05])
        // 3. getstatic MobEffects.POISON (0xB2, index 3 -> [0x00, 0x03])
        // 4. sipush 600 (0x11, [0x02, 0x58])
        // 5. iconst_1 (0x04)
        // 6. invokevirtual LivingEntity.addEffect (0xB6, index 9 -> [0x00, 0x09])
        // 7. return (0xB1)
        let bytecode = vec![
            0xB6, 0x00, 0x0F, // invokevirtual isShiftKeyDown
            0x99, 0x00, 0x05, // ifeq +5
            0xB2, 0x00, 0x03, // getstatic POISON
            0x11, 0x02, 0x58, // sipush 600
            0x04, // iconst_1
            0xB6, 0x00, 0x09, // invokevirtual addEffect
            0xB1, // return
        ];

        let method = MethodInfo {
            access_flags: 0x0001,
            name_index: 0,
            name: "finishUsingItem".into(),
            descriptor_index: 0,
            descriptor: "()V".into(),
            code: Some(CodeInfo {
                max_stack: 4,
                max_locals: 4,
                bytecode,
            }),
            attributes: vec![],
        };

        let class = create_test_class("com/example/PoisonApple", vec![method], cp);
        let action = extract_item_action(class.methods(), &class, "example:poison_apple");

        assert!(action.is_some());
        let def = action.unwrap();
        assert_eq!(def.trigger, ItemTrigger::Consume);
        assert!(def.actions.contains(&ActionEffect::ApplyEffect {
            effect: "minecraft:poison".into(),
            duration_ticks: 600,
            amplifier: 1,
        }));
        assert!(def
            .conditions
            .contains(&ActionCondition::Sneaking { sneaking: true }));
    }

    #[test]
    fn test_extract_block_triggers() {
        let mut cp = ConstantPool::new();
        // 1: this class
        cp.push(ConstantPoolEntry::Class { name_index: 2 });
        // 2: "com/example/TrapBlock"
        cp.push(ConstantPoolEntry::Utf8("com/example/TrapBlock".into()));
        // 3: Methodref Level.setBlock
        cp.push(ConstantPoolEntry::Methodref {
            class_index: 4,
            name_and_type_index: 5,
        });
        // 4: Level class
        cp.push(ConstantPoolEntry::Class { name_index: 6 });
        // 5: NameAndType setBlock
        cp.push(ConstantPoolEntry::NameAndType {
            name_index: 7,
            descriptor_index: 8,
        });
        // 6: "net/minecraft/world/level/Level"
        cp.push(ConstantPoolEntry::Utf8(
            "net/minecraft/world/level/Level".into(),
        ));
        // 7: "setBlock"
        cp.push(ConstantPoolEntry::Utf8("setBlock".into()));
        // 8: "()V"
        cp.push(ConstantPoolEntry::Utf8("()V".into()));
        // 9: Methodref Level.explode
        cp.push(ConstantPoolEntry::Methodref {
            class_index: 4,
            name_and_type_index: 10,
        });
        // 10: NameAndType explode
        cp.push(ConstantPoolEntry::NameAndType {
            name_index: 11,
            descriptor_index: 8,
        });
        // 11: "explode"
        cp.push(ConstantPoolEntry::Utf8("explode".into()));

        // Method 1: "use" (Interact) sets block state
        let method_use = MethodInfo {
            access_flags: 0x0001,
            name_index: 0,
            name: "use".into(),
            descriptor_index: 0,
            descriptor: "()V".into(),
            code: Some(CodeInfo {
                max_stack: 2,
                max_locals: 2,
                bytecode: vec![0xB6, 0x00, 0x03, 0xB1], // setBlock, return
            }),
            attributes: vec![],
        };

        // Method 2: "stepOn" (StepOn) explodes
        let method_step = MethodInfo {
            access_flags: 0x0001,
            name_index: 0,
            name: "stepOn".into(),
            descriptor_index: 0,
            descriptor: "()V".into(),
            code: Some(CodeInfo {
                max_stack: 2,
                max_locals: 2,
                bytecode: vec![0x0D, 0xB6, 0x00, 0x09, 0xB1], // fconst_2 (power 2.0), explode, return
            }),
            attributes: vec![],
        };

        let class = create_test_class(
            "com/example/TrapBlock",
            vec![method_use, method_step],
            cp,
        );
        let triggers = extract_block_triggers(&class, "example:trap_block");

        assert_eq!(triggers.len(), 2);
        let interact = triggers
            .iter()
            .find(|t| t.trigger == BlockTriggerType::Interact)
            .unwrap();
        assert!(interact.actions.contains(&ActionEffect::SetBlockState {
            block_id: None,
            properties: HashMap::new(),
        }));

        let step_on = triggers
            .iter()
            .find(|t| t.trigger == BlockTriggerType::StepOn)
            .unwrap();
        assert!(step_on.actions.contains(&ActionEffect::Explode {
            power: 2.0,
            causes_fire: false,
            break_blocks: true,
        }));
    }

    #[test]
    fn test_extract_helper_method_recursion() {
        let mut cp = ConstantPool::new();
        // 1: this class "com/example/Wand"
        cp.push(ConstantPoolEntry::Class { name_index: 2 });
        // 2: "com/example/Wand"
        cp.push(ConstantPoolEntry::Utf8("com/example/Wand".into()));
        // 3: helper methodref Wand.castFire
        cp.push(ConstantPoolEntry::Methodref {
            class_index: 1,
            name_and_type_index: 4,
        });
        // 4: NameAndType castFire
        cp.push(ConstantPoolEntry::NameAndType {
            name_index: 5,
            descriptor_index: 6,
        });
        // 5: "castFire"
        cp.push(ConstantPoolEntry::Utf8("castFire".into()));
        // 6: "()V"
        cp.push(ConstantPoolEntry::Utf8("()V".into()));
        // 7: Methodref Level.explode
        cp.push(ConstantPoolEntry::Methodref {
            class_index: 8,
            name_and_type_index: 9,
        });
        // 8: Level class
        cp.push(ConstantPoolEntry::Class { name_index: 10 });
        // 9: NameAndType explode
        cp.push(ConstantPoolEntry::NameAndType {
            name_index: 11,
            descriptor_index: 6,
        });
        // 10: "net/minecraft/world/level/Level"
        cp.push(ConstantPoolEntry::Utf8(
            "net/minecraft/world/level/Level".into(),
        ));
        // 11: "explode"
        cp.push(ConstantPoolEntry::Utf8("explode".into()));

        // Method 1: "use" calls helper method "castFire" (invokevirtual Wand.castFire)
        let method_use = MethodInfo {
            access_flags: 0x0001,
            name_index: 0,
            name: "use".into(),
            descriptor_index: 0,
            descriptor: "()V".into(),
            code: Some(CodeInfo {
                max_stack: 2,
                max_locals: 2,
                bytecode: vec![
                    0xB6, 0x00, 0x03, // invokevirtual Wand.castFire
                    0xB1, // return
                ],
            }),
            attributes: vec![],
        };

        // Method 2: helper "castFire" triggers explosion
        let method_cast = MethodInfo {
            access_flags: 0x0002, // private
            name_index: 0,
            name: "castFire".into(),
            descriptor_index: 0,
            descriptor: "()V".into(),
            code: Some(CodeInfo {
                max_stack: 2,
                max_locals: 2,
                bytecode: vec![
                    0x0D, // fconst_2 (power 2.0)
                    0xB6, 0x00, 0x07, // invokevirtual explode
                    0xB1, // return
                ],
            }),
            attributes: vec![],
        };

        let class = create_test_class(
            "com/example/Wand",
            vec![method_use, method_cast],
            cp,
        );
        let action = extract_item_action(class.methods(), &class, "example:wand");

        assert!(action.is_some());
        let def = action.unwrap();
        assert_eq!(def.trigger, ItemTrigger::RightClick);
        assert!(def.actions.contains(&ActionEffect::Explode {
            power: 2.0,
            causes_fire: false,
            break_blocks: true,
        }));
    }

    #[test]
    fn test_extract_hit_entity_and_healing() {
        let mut cp = ConstantPool::new();
        // 1: this class
        cp.push(ConstantPoolEntry::Class { name_index: 2 });
        // 2: "com/example/VampireSword"
        cp.push(ConstantPoolEntry::Utf8("com/example/VampireSword".into()));
        // 3: Methodref Player.heal
        cp.push(ConstantPoolEntry::Methodref {
            class_index: 4,
            name_and_type_index: 5,
        });
        // 4: Player class
        cp.push(ConstantPoolEntry::Class { name_index: 6 });
        // 5: NameAndType heal
        cp.push(ConstantPoolEntry::NameAndType {
            name_index: 7,
            descriptor_index: 8,
        });
        // 6: "net/minecraft/world/entity/player/Player"
        cp.push(ConstantPoolEntry::Utf8(
            "net/minecraft/world/entity/player/Player".into(),
        ));
        // 7: "heal"
        cp.push(ConstantPoolEntry::Utf8("heal".into()));
        // 8: "(F)V"
        cp.push(ConstantPoolEntry::Utf8("(F)V".into()));

        // Method "hurtEnemy" (HitEntity trigger) calls heal(4.0f)
        let bytecode = vec![
            0x0C, // fconst_1
            0xB6, 0x00, 0x03, // invokevirtual heal
            0xB1, // return
        ];

        let method = MethodInfo {
            access_flags: 0x0001,
            name_index: 0,
            name: "hurtEnemy".into(),
            descriptor_index: 0,
            descriptor: "()V".into(),
            code: Some(CodeInfo {
                max_stack: 2,
                max_locals: 2,
                bytecode,
            }),
            attributes: vec![],
        };

        let class = create_test_class("com/example/VampireSword", vec![method], cp);
        let action = extract_item_action(class.methods(), &class, "example:vampire_sword");

        assert!(action.is_some());
        let def = action.unwrap();
        assert_eq!(def.trigger, ItemTrigger::HitEntity);
        assert!(def.actions.contains(&ActionEffect::Heal { amount: 1.0 }));
    }

    #[test]
    fn test_extract_empty_method_returns_none() {
        let mut cp = ConstantPool::new();
        cp.push(ConstantPoolEntry::Class { name_index: 2 });
        cp.push(ConstantPoolEntry::Utf8("com/example/EmptyItem".into()));

        // Method "use" with only return
        let method = MethodInfo {
            access_flags: 0x0001,
            name_index: 0,
            name: "use".into(),
            descriptor_index: 0,
            descriptor: "()V".into(),
            code: Some(CodeInfo {
                max_stack: 1,
                max_locals: 1,
                bytecode: vec![0xB1], // return
            }),
            attributes: vec![],
        };

        let class = create_test_class("com/example/EmptyItem", vec![method], cp);
        let action = extract_item_action(class.methods(), &class, "example:empty_item");
        assert!(action.is_none());
    }
}

//! Action Dispatcher Bridge executing mod bytecode actions and semantic item effects
//! via Horizon's `Host` server and client runtime.

use horizon_client::mod_api::{Flow, Host, Text};
use rurge_converter::ir::{ActionEffect, ItemTrigger};
use rurge_runtime::engine::RurgeEngine;
use std::sync::Arc;

static WAND_MANA: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(1400);

pub struct ActionDispatcher;

impl ActionDispatcher {
    pub fn get_wand_mana() -> i64 {
        WAND_MANA.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn set_wand_mana(val: i64) {
        WAND_MANA.store(val, std::sync::atomic::Ordering::Relaxed);
    }

    /// Resolves the acting player, with automatic singleplayer fallback to the first server player.
    #[inline]
    pub fn resolve_player(host: &dyn Host) -> Option<horizon_client::mod_api::NativeServerPlayer> {
        host.acting_player().or_else(|| host.server_players().into_iter().next())
    }

    /// Attempts to execute an item action for the given item stack.
    /// Returns `Flow::Cancel` if an action was handled (consuming the click and swinging hand),
    /// or `Flow::Continue` if no action was taken.
    pub fn handle_item_use(
        engine: &Arc<RurgeEngine>,
        host: &mut dyn Host,
        item_id: &str,
        target_pos: Option<[f64; 3]>,
        target_block_pos: Option<[i32; 3]>,
    ) -> Flow {
        let trimmed_id = item_id.trim();
        if trimmed_id.is_empty() || trimmed_id.starts_with("minecraft:") {
            return Flow::Continue;
        }

        // 1. Check registered declarative actions in ContentSchemaExecutor
        let registered_actions = engine.content().get_item_actions(trimmed_id);
        let player = Self::resolve_player(host);

        // Compute player view vector and coordinates as f64
        let (dir_x, dir_y, dir_z) = if let Some(ref p) = player {
            let yaw_rad = (-(p.yaw as f64) - 90.0).to_radians();
            let pitch_rad = (-(p.pitch as f64)).to_radians();
            (
                pitch_rad.cos() * yaw_rad.cos(),
                pitch_rad.sin(),
                pitch_rad.cos() * yaw_rad.sin(),
            )
        } else {
            (0.0, 0.0, 1.0)
        };

        let eye_pos = if let Some(ref p) = player {
            [p.position[0], p.position[1] + 1.62, p.position[2]]
        } else {
            target_pos.unwrap_or([0.0, 64.0, 0.0])
        };

        let spawn_pos = [
            eye_pos[0] + dir_x * 1.5,
            eye_pos[1] + dir_y * 1.5,
            eye_pos[2] + dir_z * 1.5,
        ];

        // 2. If explicit actions exist, dispatch them
        if !registered_actions.is_empty() {
            let mut handled = false;
            for action in &registered_actions {
                if action.trigger == ItemTrigger::RightClick {
                    Self::dispatch_effects(
                        host,
                        trimmed_id,
                        &action.actions,
                        eye_pos,
                        spawn_pos,
                        [dir_x, dir_y, dir_z],
                        target_pos,
                        target_block_pos,
                    );
                    handled = true;
                }
            }
            if handled {
                return Flow::Cancel;
            }
        }

        // 3. Fall back to smart semantic deduction for magic mods and custom tools
        if Self::dispatch_semantic_action(
            host,
            trimmed_id,
            eye_pos,
            [dir_x, dir_y, dir_z],
            target_pos,
            target_block_pos,
        ) {
            return Flow::Cancel;
        }

        Flow::Continue
    }

    /// Handles right clicking on a block in world (e.g. Arcane Workbench, Altar, Pedestal).
    pub fn handle_block_use(
        engine: &Arc<RurgeEngine>,
        host: &mut dyn Host,
        block_id: &str,
        pos: [i32; 3],
    ) -> Flow {
        let trimmed_id = block_id.trim();
        if trimmed_id.is_empty() || trimmed_id.starts_with("minecraft:") {
            return Flow::Continue;
        }

        // 1. Check registered declarative block triggers
        let registered_triggers = engine.content().get_block_triggers(trimmed_id);
        if !registered_triggers.is_empty() {
            let player = Self::resolve_player(host);
            let eye_pos = player.as_ref().map(|p| [p.position[0], p.position[1] + 1.62, p.position[2]]).unwrap_or([pos[0] as f64, pos[1] as f64 + 1.0, pos[2] as f64]);
            for trigger in &registered_triggers {
                Self::dispatch_effects(
                    host,
                    trimmed_id,
                    &trigger.actions,
                    eye_pos,
                    [pos[0] as f64 + 0.5, pos[1] as f64 + 0.5, pos[2] as f64 + 0.5],
                    [0.0, 1.0, 0.0],
                    Some([pos[0] as f64 + 0.5, pos[1] as f64 + 0.5, pos[2] as f64 + 0.5]),
                    Some(pos),
                );
            }
            return Flow::Cancel;
        }

        // 2. Semantic block interactions (Arcane Workbench, Altar, Pedestals)
        if trimmed_id.contains("workbench") || trimmed_id.contains("altar") || trimmed_id.contains("pedestal") {
            WAND_MANA.store(1400, std::sync::atomic::Ordering::Relaxed);
            host.run_command(&format!(
                "playsound block.enchantment_table.use block @a {} {} {} 1.0 1.0",
                pos[0], pos[1], pos[2]
            ));
            host.run_command(&format!(
                "particle enchant {:.2} {:.2} {:.2} 0.5 0.5 0.5 0.2 25",
                pos[0] as f64 + 0.5,
                pos[1] as f64 + 1.2,
                pos[2] as f64 + 0.5
            ));
            if let Some(player) = Self::resolve_player(host) {
                host.action_bar(
                    player.uuid,
                    Text::literal(format!("§6[Wizardry]§r Interacted with {}", clean_name(trimmed_id))),
                );
            }
            return Flow::Cancel;
        }

        Flow::Continue
    }

    /// Dispatches a slice of `ActionEffect`s through `Host`.
    #[allow(clippy::too_many_arguments)]
    pub fn dispatch_effects(
        host: &mut dyn Host,
        item_or_block_id: &str,
        effects: &[ActionEffect],
        eye_pos: [f64; 3],
        spawn_pos: [f64; 3],
        direction: [f64; 3],
        target_pos: Option<[f64; 3]>,
        target_block_pos: Option<[i32; 3]>,
    ) {
        let player = Self::resolve_player(host);

        for effect in effects {
            match effect {
                ActionEffect::PlaySound { sound, volume, pitch } => {
                    let cmd = format!("playsound {sound} player @a ~ ~ ~ {volume} {pitch}");
                    let ok = host.run_command(&cmd);
                    if !ok {
                        // Vanilla fallback sound
                        host.run_command("playsound entity.blaze.shoot player @a ~ ~ ~ 0.8 1.2");
                    }
                }
                ActionEffect::SpawnEntity { entity_type, count: _ } => {
                    let vx = direction[0] * 1.8;
                    let vy = direction[1] * 1.8;
                    let vz = direction[2] * 1.8;
                    let cmd = format!(
                        "summon {entity_type} {:.2} {:.2} {:.2} {{Motion:[{:.3},{:.3},{:.3}]}}",
                        spawn_pos[0], spawn_pos[1], spawn_pos[2], vx, vy, vz
                    );
                    let ok = host.run_command(&cmd);
                    if !ok {
                        // Visual projectile raycast with particle effects
                        Self::render_particle_beam(host, eye_pos, direction, 14.0, "flame");
                        let impact = [
                            eye_pos[0] + direction[0] * 12.0,
                            eye_pos[1] + direction[1] * 12.0,
                            eye_pos[2] + direction[2] * 12.0,
                        ];
                        host.run_command(&format!(
                            "damage @e[x={:.2},y={:.2},z={:.2},distance=..3.5,type=!player] 7 magic",
                            impact[0], impact[1], impact[2]
                        ));
                    }
                }
                ActionEffect::Explode { power, causes_fire, break_blocks: _ } => {
                    let target = target_pos.unwrap_or(spawn_pos);
                    host.run_command(&format!(
                        "particle explosion_emitter {:.2} {:.2} {:.2} 0 0 0 0 1",
                        target[0], target[1], target[2]
                    ));
                    host.run_command(&format!(
                        "playsound entity.generic.explode block @a {:.2} {:.2} {:.2} {:.1} 1.0",
                        target[0], target[1], target[2], power.min(2.0)
                    ));
                    host.run_command(&format!(
                        "damage @e[x={:.2},y={:.2},z={:.2},distance=..{:.1},type=!player] {:.1} explosion",
                        target[0], target[1], target[2], power * 2.0, power * 3.5
                    ));
                    if *causes_fire {
                        host.run_command(&format!(
                            "setblock {:.0} {:.0} {:.0} fire keep",
                            target[0], target[1], target[2]
                        ));
                    }
                }
                ActionEffect::ApplyEffect { effect, duration_ticks, amplifier } => {
                    let vanilla_effect = match effect.as_str() {
                        e if e.contains("frost") => "slowness",
                        e if e.contains("fireskin") => "fire_resistance",
                        e if e.contains("decay") => "wither",
                        e if e.contains("ward") => "resistance",
                        e if e.contains("agility") => "speed",
                        e if e.contains("flight") => "levitation",
                        e if e.contains("sixth_sense") => "night_vision",
                        e if e.contains("poison") => "poison",
                        _ => "glowing",
                    };
                    let duration_secs = (*duration_ticks / 20).max(1);
                    host.run_command(&format!(
                        "effect give @p {vanilla_effect} {duration_secs} {amplifier}"
                    ));
                }
                ActionEffect::Heal { amount } => {
                    let amp = (*amount / 4.0).max(0.0) as u32;
                    host.run_command(&format!("effect give @p instant_health 1 {amp}"));
                    host.run_command("playsound entity.player.levelup player @p ~ ~ ~ 0.8 1.5");
                    host.run_command("particle heart ~ ~1.5 ~ 0.5 0.5 0.5 0.1 8");
                }
                ActionEffect::Damage { amount, damage_type } => {
                    let dtype = damage_type.as_deref().unwrap_or("magic");
                    host.run_command(&format!("damage @p {:.1} {dtype}", amount));
                }
                ActionEffect::DamageItem { amount: _ } => {}
                ActionEffect::GiveItem { item_id, count } => {
                    host.run_command(&format!("give @p {item_id} {count}"));
                }
                ActionEffect::ConsumeItem { count: _ } => {
                    host.run_command(&format!("clear @p {item_or_block_id} 1"));
                }
                ActionEffect::SendMessage { message } => {
                    if let Some(ref p) = player {
                        host.action_bar(p.uuid, Text::literal(message));
                    } else {
                        host.broadcast(Text::literal(message));
                    }
                }
                ActionEffect::Teleport { offset_x, offset_y, offset_z } => {
                    let dest = [eye_pos[0] + offset_x, eye_pos[1] + offset_y, eye_pos[2] + offset_z];
                    host.run_command(&format!("tp @p {:.2} {:.2} {:.2}", dest[0], dest[1], dest[2]));
                    host.run_command("playsound entity.enderman.teleport player @p ~ ~ ~ 1.0 1.0");
                    host.run_command("particle portal ~ ~1.0 ~ 0.5 0.5 0.5 0.1 20");
                }
                ActionEffect::SetBlockState { block_id, .. } => {
                    if let Some(bpos) = target_block_pos
                        && let Some(bid) = block_id {
                            host.run_command(&format!("setblock {} {} {} {bid}", bpos[0], bpos[1], bpos[2]));
                        }
                }
            }
        }
    }

    /// Smart semantic deduction for unknown mod items based on naming conventions.
    pub fn dispatch_semantic_action(
        host: &mut dyn Host,
        item_id: &str,
        eye_pos: [f64; 3],
        direction: [f64; 3],
        target_pos: Option<[f64; 3]>,
        _target_block_pos: Option<[i32; 3]>,
    ) -> bool {
        let lower = item_id.to_ascii_lowercase();

        // 1. Magic Wands
        if lower.contains("wand") {
            let current_mana = WAND_MANA.load(std::sync::atomic::Ordering::Relaxed);
            let (max_mana, spell_cost) = if lower.contains("master") {
                (1400, 45)
            } else if lower.contains("advanced") {
                (700, 30)
            } else if lower.contains("apprentice") {
                (300, 20)
            } else {
                (100, 10)
            };

            if current_mana < spell_cost {
                host.run_command("playsound block.fire.extinguish player @a ~ ~ ~ 0.8 1.5");
                host.run_command("particle smoke ~ ~1 ~ 0.2 0.2 0.2 0.05 12");
                if let Some(player) = Self::resolve_player(host) {
                    host.action_bar(
                        player.uuid,
                        Text::literal(format!("§c⚠️ Not enough mana! [{}/{}] Recharge at Arcane Workbench§r", current_mana, max_mana)),
                    );
                }
                return true;
            }

            let new_mana = (current_mana - spell_cost).max(0);
            WAND_MANA.store(new_mana, std::sync::atomic::Ordering::Relaxed);
            host.run_command("playsound item.wand.blast player @a ~ ~ ~ 0.9 1.1");
            let particle = if lower.contains("fire") {
                "flame"
            } else if lower.contains("ice") || lower.contains("frost") {
                "snowflake"
            } else if lower.contains("lightning") {
                "electric_spark"
            } else if lower.contains("necromancy") {
                "soul_fire_flame"
            } else if lower.contains("healing") {
                "heart"
            } else {
                "enchanted_hit"
            };

            Self::render_particle_beam(host, eye_pos, direction, 16.0, particle);

            let impact = [
                eye_pos[0] + direction[0] * 14.0,
                eye_pos[1] + direction[1] * 14.0,
                eye_pos[2] + direction[2] * 14.0,
            ];

            if lower.contains("fire") {
                host.run_command(&format!(
                    "damage @e[x={:.2},y={:.2},z={:.2},distance=..3.5,type=!player] 8 in_fire",
                    impact[0], impact[1], impact[2]
                ));
            } else if lower.contains("ice") || lower.contains("frost") {
                host.run_command(&format!(
                    "effect give @e[x={:.2},y={:.2},z={:.2},distance=..3.5,type=!player] slowness 5 2",
                    impact[0], impact[1], impact[2]
                ));
                host.run_command(&format!(
                    "damage @e[x={:.2},y={:.2},z={:.2},distance=..3.5,type=!player] 6 freeze",
                    impact[0], impact[1], impact[2]
                ));
            } else if lower.contains("lightning") {
                host.run_command(&format!(
                    "summon lightning_bolt {:.2} {:.2} {:.2}",
                    impact[0], impact[1], impact[2]
                ));
            } else if lower.contains("healing") {
                host.run_command("effect give @p instant_health 1 1");
                host.run_command("playsound entity.player.levelup player @p ~ ~ ~ 0.8 1.4");
            } else {
                host.run_command(&format!(
                    "damage @e[x={:.2},y={:.2},z={:.2},distance=..3.5,type=!player] 7 magic",
                    impact[0], impact[1], impact[2]
                ));
            }

            if let Some(player) = Self::resolve_player(host) {
                host.action_bar(
                    player.uuid,
                    Text::literal(format!("§b⚡ Mana: [{}/{}] §f| §6Cast: §e{}§r", new_mana, max_mana, clean_name(item_id))),
                );
            }
            return true;
        }

        // 2. Spell Books and Tomes
        if lower.contains("spell_book") || lower.contains("tome") {
            host.run_command("playsound block.enchantment_table.use player @a ~ ~ ~ 1.0 1.2");
            Self::render_particle_beam(host, eye_pos, direction, 12.0, "witch");
            if let Some(player) = Self::resolve_player(host) {
                host.action_bar(
                    player.uuid,
                    Text::literal(format!("§d📖 Channeled:§r {}", clean_name(item_id))),
                );
            }
            return true;
        }

        // 3. Spell Scrolls
        if lower.contains("scroll") {
            host.run_command("playsound item.book.page_turn player @a ~ ~ ~ 1.0 1.0");
            host.run_command("playsound entity.evoker.cast_spell player @a ~ ~ ~ 1.0 1.2");
            Self::render_particle_beam(host, eye_pos, direction, 14.0, "portal");
            let impact = [
                eye_pos[0] + direction[0] * 12.0,
                eye_pos[1] + direction[1] * 12.0,
                eye_pos[2] + direction[2] * 12.0,
            ];
            host.run_command(&format!(
                "damage @e[x={:.2},y={:.2},z={:.2},distance=..4,type=!player] 9 magic",
                impact[0], impact[1], impact[2]
            ));
            host.run_command(&format!("clear @p {item_id} 1"));
            if let Some(player) = Self::resolve_player(host) {
                host.action_bar(
                    player.uuid,
                    Text::literal(format!("§e📜 Inscribed:§r {}", clean_name(item_id))),
                );
            }
            return true;
        }

        // 4. Bombs
        if lower.contains("bomb") {
            let target = target_pos.unwrap_or([
                eye_pos[0] + direction[0] * 8.0,
                eye_pos[1] + direction[1] * 8.0,
                eye_pos[2] + direction[2] * 8.0,
            ]);
            host.run_command(&format!(
                "particle explosion_emitter {:.2} {:.2} {:.2} 0 0 0 0 1",
                target[0], target[1], target[2]
            ));
            host.run_command(&format!(
                "playsound entity.generic.explode block @a {:.2} {:.2} {:.2} 1.5 1.0",
                target[0], target[1], target[2]
            ));
            host.run_command(&format!(
                "damage @e[x={:.2},y={:.2},z={:.2},distance=..4.5,type=!player] 12 explosion",
                target[0], target[1], target[2]
            ));
            host.run_command(&format!("clear @p {item_id} 1"));
            return true;
        }

        // 5. Flasks and Elixirs
        if lower.contains("flask") || lower.contains("elixir") {
            host.run_command("playsound entity.generic.drink player @a ~ ~ ~ 1.0 1.0");
            host.run_command("effect give @p regeneration 10 1");
            host.run_command("effect give @p speed 15 1");
            host.run_command(&format!("clear @p {item_id} 1"));
            host.run_command("give @p minecraft:glass_bottle 1");
            if let Some(player) = Self::resolve_player(host) {
                host.action_bar(
                    player.uuid,
                    Text::literal(format!("§a🧪 Drank:§r {}", clean_name(item_id))),
                );
            }
            return true;
        }

        false
    }

    /// Renders a ray of particles along the player's view vector.
    fn render_particle_beam(
        host: &mut dyn Host,
        start: [f64; 3],
        dir: [f64; 3],
        max_dist: f64,
        particle: &str,
    ) {
        let step_size = 1.0;
        let mut dist = 1.0;
        while dist <= max_dist {
            let px = start[0] + dir[0] * dist;
            let py = start[1] + dir[1] * dist;
            let pz = start[2] + dir[2] * dist;
            host.run_command(&format!(
                "particle {particle} {:.2} {:.2} {:.2} 0.05 0.05 0.05 0.02 4",
                px, py, pz
            ));
            dist += step_size;
        }
    }
}

fn clean_name(id: &str) -> String {
    let name = id.split_once(':').map_or(id, |(_, p)| p);
    name.split('_')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

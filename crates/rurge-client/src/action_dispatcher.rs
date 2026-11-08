//! Action Dispatcher Bridge executing mod bytecode actions and semantic item effects
//! via Horizon's `Host` server and client runtime.

use horizon_client::mod_api::{Flow, Host, Text};
use rurge_converter::ir::{ActionEffect, ItemTrigger};
use rurge_runtime::engine::RurgeEngine;
use std::sync::Arc;

static WAND_MANA: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(1400);
static WAND_ACTIVE_SPELL_IDX: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

const WIZARDRY_SPELL_LIST: &[(&str, &str, i64)] = &[
    ("zombie_apocalypse", "§5☠ Zombie Apocalypse", 150),
    ("fireball", "§c🔥 Fireball", 35),
    ("lightning_bolt", "§e⚡ Lightning Bolt", 40),
    ("ice_shard", "§b❄ Ice Shard", 25),
    ("heal", "§a💖 Divine Heal", 30),
    ("blink", "§d✨ Blink", 20),
    ("summon_zombie", "§2🧟 Summon Zombie", 40),
    ("tornado", "§7🌪 Tornado", 50),
    ("fangs", "§8🐊 Evoker Fangs", 35),
    ("flight", "§f🕊 Arcane Flight", 45),
];

pub struct ActionDispatcher;

impl ActionDispatcher {
    pub fn get_wand_mana() -> i64 {
        WAND_MANA.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn set_wand_mana(val: i64) {
        WAND_MANA.store(val, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn get_active_wand_spell() -> &'static (&'static str, &'static str, i64) {
        let idx = WAND_ACTIVE_SPELL_IDX.load(std::sync::atomic::Ordering::Relaxed);
        &WIZARDRY_SPELL_LIST[idx % WIZARDRY_SPELL_LIST.len()]
    }

    pub fn cycle_wand_spell() -> &'static (&'static str, &'static str, i64) {
        let next = WAND_ACTIVE_SPELL_IDX.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        &WIZARDRY_SPELL_LIST[next % WIZARDRY_SPELL_LIST.len()]
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

        log::info!("[ActionDispatcher] Player used mod item: '{}'", trimmed_id);

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

        // 1. Check registered declarative actions in ContentSchemaExecutor
        let registered_actions = engine.content().get_item_actions(trimmed_id);

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

        log::info!("[ActionDispatcher] Player interacted with mod block: '{}' at {:?}", trimmed_id, pos);

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
                ActionEffect::SpawnEntity { entity_type, count } => {
                    let c = (*count).max(1);
                    let base_target = target_pos.unwrap_or(spawn_pos);
                    let mut any_spawned = false;
                    for i in 0..c {
                        let offset_x = if c > 1 { ((i as f64 * (std::f64::consts::TAU / c as f64)).cos()) * 2.5 } else { 0.0 };
                        let offset_z = if c > 1 { ((i as f64 * (std::f64::consts::TAU / c as f64)).sin()) * 2.5 } else { 0.0 };
                        let sx = base_target[0] + offset_x;
                        let sy = base_target[1];
                        let sz = base_target[2] + offset_z;

                        let resolved = resolve_entity_command(entity_type);
                        let cmd = format!("summon {resolved} {:.2} {:.2} {:.2}", sx, sy, sz);
                        if host.run_command(&cmd) {
                            any_spawned = true;
                        }
                    }
                    if any_spawned {
                        host.run_command(&format!(
                            "particle portal {:.2} {:.2} {:.2} 1.2 0.8 1.2 0.1 30",
                            base_target[0], base_target[1] + 1.0, base_target[2]
                        ));
                    } else {
                        // Visual projectile raycast with particle effects fallback
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
                    let dest = if let Some(tpos) = target_pos {
                        [tpos[0], tpos[1] + 1.0, tpos[2]]
                    } else {
                        [eye_pos[0] + offset_x, eye_pos[1] + offset_y, eye_pos[2] + offset_z]
                    };
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

            log::info!("[ActionDispatcher] Casting wand: '{}' (mana: {}/{}, cost: {})", item_id, current_mana, max_mana, spell_cost);

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
            } else if lower.contains("necromancy") || lower.contains("zombie") {
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
                    Text::literal(format!("§b⚡ Cast Wand: [{}/{}] §f| §6Spell: §e{}§r", new_mana, max_mana, clean_name(item_id))),
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

            let impact = target_pos.unwrap_or([
                eye_pos[0] + direction[0] * 12.0,
                eye_pos[1] + direction[1] * 12.0,
                eye_pos[2] + direction[2] * 12.0,
            ]);

            // Specialized spell mechanics if scroll is named
            if lower.contains("zombie_apocalypse") {
                host.run_command(&format!(
                    "playsound block.portal.travel player @a {:.2} {:.2} {:.2} 1.0 0.7",
                    impact[0], impact[1], impact[2]
                ));
                host.run_command(&format!(
                    "playsound entity.zombie.ambient player @a {:.2} {:.2} {:.2} 1.2 0.8",
                    impact[0], impact[1], impact[2]
                ));
                host.run_command(&format!(
                    "particle portal {:.2} {:.2} {:.2} 2.0 1.0 2.0 0.2 80",
                    impact[0], impact[1] + 1.0, impact[2]
                ));
                host.run_command(&format!(
                    "particle soul_fire_flame {:.2} {:.2} {:.2} 1.5 0.5 1.5 0.05 40",
                    impact[0], impact[1] + 0.5, impact[2]
                ));
                for i in 0..6 {
                    let angle = i as f64 * (std::f64::consts::TAU / 6.0);
                    let zx = impact[0] + angle.cos() * 2.8;
                    let zz = impact[2] + angle.sin() * 2.8;
                    let zy = impact[1];
                    host.run_command(&format!("summon zombie {:.2} {:.2} {:.2}", zx, zy, zz));
                }
            } else if lower.contains("summon_zombie") {
                host.run_command(&format!("summon zombie {:.2} {:.2} {:.2}", impact[0], impact[1], impact[2]));
                host.run_command(&format!("particle portal {:.2} {:.2} {:.2} 0.5 1.0 0.5 0.1 30", impact[0], impact[1] + 1.0, impact[2]));
                host.run_command(&format!("playsound entity.zombie.ambient player @a {:.2} {:.2} {:.2} 1.0 1.0", impact[0], impact[1], impact[2]));
            } else if lower.contains("lightning") {
                host.run_command(&format!("summon lightning_bolt {:.2} {:.2} {:.2}", impact[0], impact[1], impact[2]));
            } else if lower.contains("fireball") {
                host.run_command(&format!("particle explosion_emitter {:.2} {:.2} {:.2} 0 0 0 0 1", impact[0], impact[1], impact[2]));
                host.run_command(&format!("playsound entity.generic.explode block @a {:.2} {:.2} {:.2} 1.5 1.0", impact[0], impact[1], impact[2]));
            }

            Self::render_particle_beam(host, eye_pos, direction, 14.0, "portal");
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

        // 2. Bombs
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

        // 3. Flasks and Elixirs
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

    /// Dispatches specialized Electroblob's Wizardry items (Scrolls, Spell Books, Wands).
    #[allow(clippy::too_many_arguments)]
    pub fn dispatch_wizardry_item(
        host: &mut dyn Host,
        item_id: &str,
        eye_pos: [f64; 3],
        spawn_pos: [f64; 3],
        direction: [f64; 3],
        target_pos: Option<[f64; 3]>,
        _target_block_pos: Option<[i32; 3]>,
    ) -> bool {
        let lower = item_id.to_ascii_lowercase();

        // 1. Wands
        if lower.contains("wand") {
            let current_mana = WAND_MANA.load(std::sync::atomic::Ordering::Relaxed);
            let (max_mana, default_cost) = if lower.contains("master") {
                (1400, 45)
            } else if lower.contains("advanced") {
                (700, 30)
            } else if lower.contains("apprentice") {
                (300, 20)
            } else {
                (100, 10)
            };

            let (spell_id, _spell_display, spell_cost) = Self::get_active_wand_spell();
            let effective_cost = (*spell_cost).min(max_mana).max(default_cost);

            if current_mana < effective_cost {
                host.run_command("playsound block.fire.extinguish player @a ~ ~ ~ 0.8 1.5");
                host.run_command("particle smoke ~ ~1 ~ 0.2 0.2 0.2 0.05 12");
                if let Some(player) = Self::resolve_player(host) {
                    host.action_bar(
                        player.uuid,
                        Text::literal(format!("§c⚠️ Insufficient Mana: [{current_mana}/{max_mana}] (Needs {effective_cost})§r")),
                    );
                }
                return true;
            }

            let new_mana = current_mana - effective_cost;
            WAND_MANA.store(new_mana, std::sync::atomic::Ordering::Relaxed);

            Self::dispatch_wizardry_spell(
                host,
                spell_id,
                eye_pos,
                spawn_pos,
                direction,
                target_pos,
                false,
                Some((new_mana, max_mana)),
            );
            return true;
        }

        // 2. Spell Scrolls
        if lower.contains("scroll") && !lower.contains("blank") {
            let spell = Self::extract_spell_name(&lower);
            Self::dispatch_wizardry_spell(
                host,
                &spell,
                eye_pos,
                spawn_pos,
                direction,
                target_pos,
                true,
                None,
            );
            host.run_command(&format!("clear @p {item_id} 1"));
            return true;
        }

        // 3. Spell Books
        if lower.contains("spell_book") || lower.contains("tome") {
            let spell = Self::extract_spell_name(&lower);
            Self::dispatch_wizardry_spell(
                host,
                &spell,
                eye_pos,
                spawn_pos,
                direction,
                target_pos,
                false,
                None,
            );
            return true;
        }

        false
    }

    /// Executes the full, authentic spell mechanics for any Electroblob's Wizardry spell.
    #[allow(clippy::too_many_arguments)]
    pub fn dispatch_wizardry_spell(
        host: &mut dyn Host,
        spell: &str,
        eye_pos: [f64; 3],
        _spawn_pos: [f64; 3],
        direction: [f64; 3],
        target_pos: Option<[f64; 3]>,
        _is_scroll: bool,
        wand_mana_info: Option<(i64, i64)>,
    ) {
        let lower = spell.to_ascii_lowercase();
        let target = target_pos.unwrap_or([
            eye_pos[0] + direction[0] * 12.0,
            eye_pos[1] + direction[1] * 12.0,
            eye_pos[2] + direction[2] * 12.0,
        ]);

        host.run_command("playsound item.book.page_turn player @a ~ ~ ~ 0.8 1.2");

        // 1. Necromancy: Zombie Apocalypse (Master Spell)
        if lower.contains("zombie_apocalypse") {
            host.run_command("playsound entity.evoker.cast_spell player @a ~ ~ ~ 1.0 0.8");
            host.run_command(&format!(
                "playsound block.portal.travel player @a {:.2} {:.2} {:.2} 1.0 0.7",
                target[0], target[1], target[2]
            ));
            host.run_command(&format!(
                "playsound entity.zombie.ambient player @a {:.2} {:.2} {:.2} 1.2 0.8",
                target[0], target[1], target[2]
            ));

            host.run_command(&format!(
                "particle portal {:.2} {:.2} {:.2} 2.0 1.0 2.0 0.2 80",
                target[0], target[1] + 1.0, target[2]
            ));
            host.run_command(&format!(
                "particle soul_fire_flame {:.2} {:.2} {:.2} 1.5 0.5 1.5 0.05 40",
                target[0], target[1] + 0.5, target[2]
            ));

            for i in 0..6 {
                let angle = i as f64 * (std::f64::consts::TAU / 6.0);
                let zx = target[0] + angle.cos() * 2.8;
                let zz = target[2] + angle.sin() * 2.8;
                let zy = target[1];
                let cmd = format!("summon zombie {:.2} {:.2} {:.2}", zx, zy, zz);
                host.run_command(&cmd);
            }

            host.run_command(&format!(
                "damage @e[x={:.2},y={:.2},z={:.2},distance=..6,type=!zombie,type=!player] 12 magic",
                target[0], target[1], target[2]
            ));

            Self::show_spell_feedback(host, "§5☠ Zombie Apocalypse Awakened! §8[Necromancy - Master]", wand_mana_info);
            return;
        }

        // 2. Summon Zombie
        if lower.contains("summon_zombie") {
            host.run_command(&format!("summon zombie {:.2} {:.2} {:.2}", target[0], target[1], target[2]));
            host.run_command(&format!("particle portal {:.2} {:.2} {:.2} 0.5 1.0 0.5 0.1 30", target[0], target[1] + 1.0, target[2]));
            host.run_command(&format!("playsound entity.zombie.ambient player @a {:.2} {:.2} {:.2} 1.0 1.0", target[0], target[1], target[2]));
            Self::show_spell_feedback(host, "§2🧟 Summoned Minion Zombie §8[Necromancy - Novice]", wand_mana_info);
            return;
        }

        // 3. Summon Skeleton / Skeleton Legion
        if lower.contains("summon_skeleton") {
            let count = if lower.contains("legion") { 4 } else { 1 };
            for i in 0..count {
                let ox = if count > 1 { (i as f64 - 1.5) * 1.5 } else { 0.0 };
                host.run_command(&format!("summon skeleton {:.2} {:.2} {:.2}", target[0] + ox, target[1], target[2]));
            }
            host.run_command(&format!("playsound entity.skeleton.ambient player @a {:.2} {:.2} {:.2} 1.0 1.0", target[0], target[1], target[2]));
            Self::show_spell_feedback(host, "§7🏹 Summoned Skeleton Minion(s) §8[Necromancy]", wand_mana_info);
            return;
        }

        // 4. Summon Blaze
        if lower.contains("summon_blaze") {
            host.run_command(&format!("summon blaze {:.2} {:.2} {:.2}", target[0], target[1], target[2]));
            host.run_command(&format!("particle flame {:.2} {:.2} {:.2} 0.5 1.0 0.5 0.1 25", target[0], target[1] + 1.0, target[2]));
            host.run_command(&format!("playsound entity.blaze.ambient player @a {:.2} {:.2} {:.2} 1.0 1.0", target[0], target[1], target[2]));
            Self::show_spell_feedback(host, "§6🔥 Summoned Minion Blaze §8[Fire - Advanced]", wand_mana_info);
            return;
        }

        // 5. Summon Ice Giant
        if lower.contains("summon_ice_giant") || lower.contains("snow_golem") {
            let entity = if lower.contains("ice_giant") { "iron_golem" } else { "snow_golem" };
            host.run_command(&format!("summon {entity} {:.2} {:.2} {:.2}", target[0], target[1], target[2]));
            host.run_command(&format!("particle snowflake {:.2} {:.2} {:.2} 0.8 1.5 0.8 0.1 40", target[0], target[1] + 1.0, target[2]));
            host.run_command(&format!("playsound block.powder_snow.break player @a {:.2} {:.2} {:.2} 1.0 0.8", target[0], target[1], target[2]));
            Self::show_spell_feedback(host, "§b❄ Summoned Frost Golem §8[Ice]", wand_mana_info);
            return;
        }

        // 6. Pyromancy: Fireball
        if lower.contains("fireball") || lower.contains("dragon_fireball") {
            Self::render_particle_beam(host, eye_pos, direction, 14.0, "flame");
            host.run_command(&format!(
                "particle explosion_emitter {:.2} {:.2} {:.2} 0 0 0 0 1",
                target[0], target[1], target[2]
            ));
            host.run_command(&format!(
                "playsound entity.generic.explode block @a {:.2} {:.2} {:.2} 1.5 1.0",
                target[0], target[1], target[2]
            ));
            host.run_command(&format!(
                "damage @e[x={:.2},y={:.2},z={:.2},distance=..4.5,type=!player] 12 fire",
                target[0], target[1], target[2]
            ));
            host.run_command(&format!(
                "setblock {:.0} {:.0} {:.0} fire keep",
                target[0], target[1], target[2]
            ));
            Self::show_spell_feedback(host, "§c🔥 Fireball Launched! §8[Fire - Apprentice]", wand_mana_info);
            return;
        }

        // 7. Explosions & Meteors
        if lower.contains("firebomb") || lower.contains("meteor") || lower.contains("detonate") || lower.contains("combustion") {
            host.run_command(&format!(
                "particle explosion_emitter {:.2} {:.2} {:.2} 0 0 0 0 1",
                target[0], target[1], target[2]
            ));
            host.run_command(&format!(
                "playsound entity.generic.explode block @a {:.2} {:.2} {:.2} 2.0 0.9",
                target[0], target[1], target[2]
            ));
            host.run_command(&format!(
                "damage @e[x={:.2},y={:.2},z={:.2},distance=..6,type=!player] 16 explosion",
                target[0], target[1], target[2]
            ));
            Self::show_spell_feedback(host, "§4💥 Meteor Impact! §8[Fire - Master]", wand_mana_info);
            return;
        }

        // 8. Electromancy: Lightning Bolt & Thunderbolt
        if lower.contains("lightning") || lower.contains("thunder") {
            host.run_command(&format!("summon lightning_bolt {:.2} {:.2} {:.2}", target[0], target[1], target[2]));
            host.run_command(&format!(
                "damage @e[x={:.2},y={:.2},z={:.2},distance=..5,type=!player] 10 lightning_bolt",
                target[0], target[1], target[2]
            ));
            Self::show_spell_feedback(host, "§e⚡ Lightning Bolt Smite! §8[Lightning]", wand_mana_info);
            return;
        }

        // 9. Cryomancy: Ice Shard, Ice Lance, Freeze
        if lower.contains("ice") || lower.contains("frost") || lower.contains("freeze") || lower.contains("blizzard") {
            Self::render_particle_beam(host, eye_pos, direction, 14.0, "snowflake");
            host.run_command(&format!(
                "particle snowflake {:.2} {:.2} {:.2} 1.5 1.0 1.5 0.1 50",
                target[0], target[1], target[2]
            ));
            host.run_command(&format!(
                "playsound block.powder_snow.break block @a {:.2} {:.2} {:.2} 1.0 0.8",
                target[0], target[1], target[2]
            ));
            host.run_command(&format!(
                "effect give @e[x={:.2},y={:.2},z={:.2},distance=..5,type=!player] slowness 8 3",
                target[0], target[1], target[2]
            ));
            host.run_command(&format!(
                "damage @e[x={:.2},y={:.2},z={:.2},distance=..5,type=!player] 8 freeze",
                target[0], target[1], target[2]
            ));
            Self::show_spell_feedback(host, "§b❄ Piercing Frost! §8[Ice]", wand_mana_info);
            return;
        }

        // 10. Healing: Heal, Greater Heal
        if lower.contains("heal") || lower.contains("vitality") {
            host.run_command("effect give @p instant_health 1 2");
            host.run_command("playsound entity.player.levelup player @p ~ ~ ~ 1.0 1.5");
            host.run_command("particle heart ~ ~1.5 ~ 0.5 0.5 0.5 0.1 12");
            Self::show_spell_feedback(host, "§a💖 Divine Healing! §8[Healing]", wand_mana_info);
            return;
        }

        // 11. Sorcery: Blink & Teleport
        if lower.contains("blink") || lower.contains("teleport") || lower.contains("phase_step") {
            let dest = target_pos.unwrap_or([
                eye_pos[0] + direction[0] * 8.0,
                eye_pos[1] + direction[1] * 8.0,
                eye_pos[2] + direction[2] * 8.0,
            ]);
            host.run_command(&format!("tp @p {:.2} {:.2} {:.2}", dest[0], dest[1], dest[2]));
            host.run_command("playsound entity.enderman.teleport player @p ~ ~ ~ 1.0 1.0");
            host.run_command("particle portal ~ ~1.0 ~ 0.5 0.5 0.5 0.1 30");
            Self::show_spell_feedback(host, "§d✨ Blinked! §8[Sorcery]", wand_mana_info);
            return;
        }

        // 12. Sorcery: Flight & Levitation
        if lower.contains("flight") || lower.contains("levitation") {
            host.run_command("effect give @p levitation 10 1");
            host.run_command("effect give @p slow_falling 15 1");
            host.run_command("playsound entity.phantom.flap player @a ~ ~ ~ 1.0 1.2");
            Self::show_spell_feedback(host, "§f🕊 Arcane Flight! §8[Sorcery]", wand_mana_info);
            return;
        }

        // 13. Earth: Fangs
        if lower.contains("fangs") {
            for i in 1..=4 {
                let fx = eye_pos[0] + direction[0] * (i as f64 * 2.0);
                let fy = eye_pos[1];
                let fz = eye_pos[2] + direction[2] * (i as f64 * 2.0);
                host.run_command(&format!("summon evoker_fangs {:.2} {:.2} {:.2}", fx, fy, fz));
            }
            host.run_command("playsound entity.evoker_fangs.attack player @a ~ ~ ~ 1.0 1.0");
            Self::show_spell_feedback(host, "§8🐊 Evoker Fangs Erupted! §8[Earth]", wand_mana_info);
            return;
        }

        // 14. Earth: Tornado & Whirlwind
        if lower.contains("tornado") || lower.contains("whirlwind") {
            host.run_command(&format!("particle cloud {:.2} {:.2} {:.2} 1.0 2.0 1.0 0.2 60", target[0], target[1], target[2]));
            host.run_command(&format!("playsound entity.warden.sonic_boom player @a {:.2} {:.2} {:.2} 0.8 1.5", target[0], target[1], target[2]));
            host.run_command(&format!("effect give @e[x={:.2},y={:.2},z={:.2},distance=..5,type=!player] levitation 4 3", target[0], target[1], target[2]));
            host.run_command(&format!("damage @e[x={:.2},y={:.2},z={:.2},distance=..5,type=!player] 8 magic", target[0], target[1], target[2]));
            Self::show_spell_feedback(host, "§7🌪 Whirlwind Summoned! §8[Earth]", wand_mana_info);
            return;
        }

        // Generic Spell Fallback
        Self::render_particle_beam(host, eye_pos, direction, 14.0, "witch");
        host.run_command(&format!(
            "damage @e[x={:.2},y={:.2},z={:.2},distance=..4,type=!player] 8 magic",
            target[0], target[1], target[2]
        ));
        let title = format!("§d🔮 Cast: §e{}§r", clean_name(spell));
        Self::show_spell_feedback(host, &title, wand_mana_info);
    }

    /// Extracts the canonical spell ID from an item ID string.
    pub fn extract_spell_name(item_id: &str) -> String {
        let sub = item_id.split_once(':').map_or(item_id, |(_, s)| s);
        if let Some(spell) = sub.strip_prefix("scroll_") {
            spell.to_string()
        } else if let Some(spell) = sub.strip_prefix("spell_book_") {
            spell.to_string()
        } else {
            sub.to_string()
        }
    }

    fn show_spell_feedback(host: &mut dyn Host, message: &str, wand_mana_info: Option<(i64, i64)>) {
        if let Some(player) = Self::resolve_player(host) {
            let text = if let Some((cur, max)) = wand_mana_info {
                format!("{message} §f| §b[{cur}/{max} Mana]§r")
            } else {
                message.to_string()
            };
            host.action_bar(player.uuid, Text::literal(text));
        }
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

fn resolve_entity_command(entity_type: &str) -> String {
    let lower = entity_type.to_ascii_lowercase();
    if lower.contains("zombie") {
        "zombie".to_string()
    } else if lower.contains("wither_skeleton") {
        "wither_skeleton".to_string()
    } else if lower.contains("skeleton") {
        "skeleton".to_string()
    } else if lower.contains("blaze") {
        "blaze".to_string()
    } else if lower.contains("spider") {
        "cave_spider".to_string()
    } else if lower.contains("silverfish") {
        "silverfish".to_string()
    } else if lower.contains("vex") {
        "vex".to_string()
    } else if lower.contains("slime") {
        "slime".to_string()
    } else if lower.contains("snow_golem") {
        "snow_golem".to_string()
    } else if lower.contains("golem") || lower.contains("giant") {
        "iron_golem".to_string()
    } else if lower.contains("lightning") {
        "lightning_bolt".to_string()
    } else if lower.contains("fireball") {
        "small_fireball".to_string()
    } else if lower.contains("fangs") {
        "evoker_fangs".to_string()
    } else if lower.contains("phantom") {
        "phantom".to_string()
    } else {
        entity_type.split_once(':').map_or(entity_type, |(_, s)| s).to_string()
    }
}

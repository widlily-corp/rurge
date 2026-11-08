//! Multi-loader pattern matcher and synthesizer for custom mod Spells (such as Electroblob's Wizardry).
//!
//! Synthesizes concrete Spell Book and Scroll items, assigns casting actions, generates rich
//! lore tooltips with elements, tiers, and descriptions, and enriches wands and magical armor.

use crate::bytecode::ClassFile;
use crate::ir::{ActionEffect, ContentIR, ItemActionDef, ItemDef, ItemTrigger};
use crate::manifest::ModMetadata;
use std::collections::BTreeSet;

/// Scans classes and metadata for custom spell definitions and expands them into concrete spell items.
pub fn scan_spells(classes: &[ClassFile], meta: &ModMetadata, ir: &mut ContentIR) {
    let modid = &meta.id;
    let mut spell_ids = BTreeSet::new();

    // 1. Scan lang keys for `spell.<modid>.<name>` (Canonical primary source)
    let spell_prefix = format!("spell.{modid}.");
    for key in ir.lang.keys() {
        if let Some(sub) = key.strip_prefix(&spell_prefix)
            && !sub.contains('.')
            && !["none", "disabled", "discover", "resist", "conjure_item"].contains(&sub)
        {
            spell_ids.insert(sub.to_string());
        }
    }

    // 2. Fallback: If no lang keys found, scan bytecode classes for spell fields
    if spell_ids.is_empty() {
        for class in classes {
            let class_name = class.class_name();
            if class_name.contains("Spells") || class_name.contains("SpellRegistry") {
                for field in &class.fields {
                    if field.descriptor.contains("Spell") && !field.name.starts_with('$') {
                        let field_name = field.name.to_ascii_lowercase();
                        if field_name.len() >= 3 && !field_name.contains("none") {
                            spell_ids.insert(field_name);
                        }
                    }
                }
            }
        }
    }

    let mut new_items = Vec::new();
    let mut new_actions = Vec::new();
    let mut new_lang = Vec::new();

    for spell in spell_ids {
        let book_id = format!("{modid}:spell_book_{spell}");
        let scroll_id = format!("{modid}:scroll_{spell}");

        // Derive friendly display name
        let lang_key = format!("spell.{modid}.{spell}");
        let spell_name = ir.lang.get(&lang_key).cloned().unwrap_or_else(|| {
            spell
                .split('_')
                .map(|w| {
                    let mut c = w.chars();
                    match c.next() {
                        None => String::new(),
                        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        });

        // Determine element and tier heuristics
        let (elem_name, elem_col) = infer_element(&spell);
        let (tier_name, tier_col) = infer_tier(&spell);

        // Extract and wrap spell description
        let desc_key = format!("spell.{modid}.{spell}.desc");
        let desc_lines = wrap_text(ir.lang.get(&desc_key).map(String::as_str), 42);

        // 1. Spell Book
        if !ir.items.iter().any(|i| i.id == book_id) {
            let mut book = ItemDef::new(&book_id);
            book.max_stack_size = 1;
            book.rarity = match tier_name {
                "Master" => "EPIC",
                "Advanced" => "RARE",
                "Apprentice" => "UNCOMMON",
                _ => "COMMON",
            }.to_string();

            let mut book_lore = vec![
                format!("§9Element: {elem_col}{elem_name}§r"),
                format!("§6Tier: {tier_col}{tier_name}§r"),
            ];
            if !desc_lines.is_empty() {
                book_lore.push(String::new());
                book_lore.extend(desc_lines.clone());
            }
            book_lore.push(String::new());
            book_lore.push("§8Can be bound to a wand at an Arcane Workbench§r".to_string());

            if let Ok(lore_val) = serde_json::to_value(&book_lore) {
                book.components.insert("minecraft:lore".to_string(), lore_val);
            }

            let mut custom_data = serde_json::Map::new();
            custom_data.insert("spell".to_string(), serde_json::json!(format!("{modid}:{spell}")));
            custom_data.insert("element".to_string(), serde_json::json!(elem_name.to_lowercase()));
            custom_data.insert("tier".to_string(), serde_json::json!(tier_name.to_lowercase()));
            book.components.insert("minecraft:custom_data".to_string(), serde_json::Value::Object(custom_data));

            // Give spell books an active channeling action so the player can cast from books as well
            let book_effects = synthesize_spell_actions(modid, &spell, false);
            let book_action = ItemActionDef::new(ItemTrigger::RightClick, book_effects)
                .with_item_id(book_id.clone())
                .with_cooldown(30);
            book.actions.push(book_action.clone());
            new_actions.push(book_action);

            new_items.push(book);
            new_lang.push((format!("item.{modid}.spell_book_{spell}"), format!("Spell Book: {spell_name}")));
        }

        // 2. Scroll
        if !ir.items.iter().any(|i| i.id == scroll_id) {
            let mut scroll = ItemDef::new(&scroll_id);
            scroll.max_stack_size = 16;
            scroll.rarity = "COMMON".to_string();

            let mut scroll_lore = vec![
                format!("§9Element: {elem_col}{elem_name}§r"),
                format!("§6Tier: {tier_col}{tier_name}§r"),
                "§eRight-click to cast spell (Single Use)§r".to_string(),
            ];
            if !desc_lines.is_empty() {
                scroll_lore.push(String::new());
                scroll_lore.extend(desc_lines);
            }

            if let Ok(lore_val) = serde_json::to_value(&scroll_lore) {
                scroll.components.insert("minecraft:lore".to_string(), lore_val);
            }

            let mut custom_data = serde_json::Map::new();
            custom_data.insert("spell".to_string(), serde_json::json!(format!("{modid}:{spell}")));
            custom_data.insert("element".to_string(), serde_json::json!(elem_name.to_lowercase()));
            custom_data.insert("tier".to_string(), serde_json::json!(tier_name.to_lowercase()));
            scroll.components.insert("minecraft:custom_data".to_string(), serde_json::Value::Object(custom_data));

            // Build authentic spell actions
            let scroll_effects = synthesize_spell_actions(modid, &spell, true);
            let action = ItemActionDef::new(ItemTrigger::RightClick, scroll_effects)
                .with_item_id(scroll_id.clone())
                .with_cooldown(20);

            scroll.actions.push(action.clone());
            new_actions.push(action);
            new_items.push(scroll);
            new_lang.push((format!("item.{modid}.scroll_{spell}"), format!("Scroll: {spell_name}")));
        }
    }

    for item in new_items {
        ir.add_item(item);
    }
    for action in new_actions {
        ir.add_item_action(action);
    }
    for (k, v) in new_lang {
        ir.lang.insert(k, v);
    }

    // 3. Enrich existing items (Wands, Robes, Artifacts, Workbench) with rich lore
    enrich_existing_content(ir);
}

/// Infers the magical element name and Minecraft formatting code for a given spell or item name.
fn infer_element(name: &str) -> (&'static str, &'static str) {
    let lower = name.to_ascii_lowercase();
    if lower.contains("fire") || lower.contains("flame") || lower.contains("pyro") || lower.contains("blaze") || lower.contains("combustion") || lower.contains("ignite") || lower.contains("helios") || lower.contains("phoenix") {
        ("Fire", "§c")
    } else if lower.contains("ice") || lower.contains("frost") || lower.contains("freeze") || lower.contains("blizzard") || lower.contains("snow") || lower.contains("hail") || lower.contains("glacial") || lower.contains("permafrost") {
        ("Ice", "§b")
    } else if lower.contains("lightning") || lower.contains("thunder") || lower.contains("shock") || lower.contains("spark") || lower.contains("storm") || lower.contains("charge") || lower.contains("static") {
        ("Lightning", "§e")
    } else if lower.contains("heal") || lower.contains("cure") || lower.contains("ward") || lower.contains("purif") || lower.contains("shield") || lower.contains("radiant") || lower.contains("vitality") || lower.contains("resurrection") {
        ("Healing", "§a")
    } else if lower.contains("dark") || lower.contains("shadow") || lower.contains("decay") || lower.contains("drain") || lower.contains("wither") || lower.contains("poison") || lower.contains("necromanc") || lower.contains("undead") || lower.contains("plague") || lower.contains("curse") || lower.contains("soul") {
        ("Necromancy", "§8")
    } else if lower.contains("earth") || lower.contains("rock") || lower.contains("stone") || lower.contains("quake") || lower.contains("sand") || lower.contains("boulder") || lower.contains("shatter") || lower.contains("tremor") {
        ("Earth", "§6")
    } else {
        ("Sorcery", "§d")
    }
}

/// Infers the spell progression tier name and formatting color.
fn infer_tier(name: &str) -> (&'static str, &'static str) {
    let lower = name.to_ascii_lowercase();
    if lower.contains("master") || lower.contains("apocalypse") || lower.contains("cataclysm") || lower.contains("supernova") || lower.contains("titan") || lower.contains("black_hole") || lower.contains("meteor") {
        ("Master", "§6")
    } else if lower.contains("advanced") || lower.contains("chain") || lower.contains("blizzard") || lower.contains("lightning_bolt") || lower.contains("tornado") || lower.contains("earthquake") {
        ("Advanced", "§5")
    } else if lower.contains("apprentice") || lower.contains("arrow_rain") || lower.contains("fireball") || lower.contains("ice_lance") || lower.contains("bubble") || lower.contains("force") {
        ("Apprentice", "§b")
    } else {
        ("Novice", "§f")
    }
}

/// Wraps text into lines with a maximum column width.
fn wrap_text(text: Option<&str>, max_width: usize) -> Vec<String> {
    let Some(text) = text else {
        return Vec::new();
    };
    let mut lines = Vec::new();
    let mut current_line = String::new();
    for word in text.split_whitespace() {
        if current_line.len() + word.len() + 1 > max_width {
            lines.push(format!("§7{current_line}"));
            current_line = word.to_string();
        } else {
            if !current_line.is_empty() {
                current_line.push(' ');
            }
            current_line.push_str(word);
        }
    }
    if !current_line.is_empty() {
        lines.push(format!("§7{current_line}"));
    }
    lines
}

/// Enriches mod wands, wizard armors, and workbench blocks with canonical tooltip lore.
fn enrich_existing_content(ir: &mut ContentIR) {
    for item in &mut ir.items {
        let lower = item.id.to_ascii_lowercase();

        // 1. Wands
        if lower.contains("wand") && !item.components.contains_key("minecraft:lore") {
            let (tier, tier_col, max_mana) = if lower.contains("master") {
                ("Master", "§6", 1400)
            } else if lower.contains("advanced") {
                ("Advanced", "§5", 700)
            } else if lower.contains("apprentice") {
                ("Apprentice", "§b", 300)
            } else {
                ("Novice", "§f", 100)
            };
            let (elem, elem_col) = infer_element(&lower);
            let lore = vec![
                format!("§6Tier: {tier_col}{tier}§r"),
                format!("§9Element: {elem_col}{elem}§r"),
                format!("§bMana: {max_mana}/{max_mana}§r"),
                "§7Right-click to channel magic§r".to_string(),
                "§8Upgrade at an Arcane Workbench§r".to_string(),
            ];
            if let Ok(v) = serde_json::to_value(&lore) {
                item.components.insert("minecraft:lore".to_string(), v);
            }
            let mut custom_data = serde_json::Map::new();
            custom_data.insert("mana".to_string(), serde_json::json!(max_mana));
            custom_data.insert("max_mana".to_string(), serde_json::json!(max_mana));
            custom_data.insert("tier".to_string(), serde_json::json!(tier.to_lowercase()));
            custom_data.insert("element".to_string(), serde_json::json!(elem.to_lowercase()));
            item.components.insert("minecraft:custom_data".to_string(), serde_json::Value::Object(custom_data));
        }
        // 2. Wizard Robes & Armor
        else if (lower.contains("wizard_hat") || lower.contains("wizard_robe") || lower.contains("wizard_leggings") || lower.contains("wizard_boots"))
            && !item.components.contains_key("minecraft:lore")
        {
            let (elem, elem_col) = infer_element(&lower);
            let lore = vec![
                format!("§9Element: {elem_col}{elem}§r"),
                "§a+15% Magic Damage§r".to_string(),
                "§b-10% Mana Cost§r".to_string(),
            ];
            if let Ok(v) = serde_json::to_value(&lore) {
                item.components.insert("minecraft:lore".to_string(), v);
            }
            let mut custom_data = serde_json::Map::new();
            custom_data.insert("element".to_string(), serde_json::json!(elem.to_lowercase()));
            item.components.insert("minecraft:custom_data".to_string(), serde_json::Value::Object(custom_data));
        }
        // 3. Blank Books & Scrolls
        else if lower.contains("blank_scroll") && !item.components.contains_key("minecraft:lore") {
            let lore = vec![
                "§7Can be inscribed with a spell at an Arcane Workbench§r".to_string(),
            ];
            if let Ok(v) = serde_json::to_value(&lore) {
                item.components.insert("minecraft:lore".to_string(), v);
            }
            let mut custom_data = serde_json::Map::new();
            custom_data.insert("blank_scroll".to_string(), serde_json::json!(true));
            item.components.insert("minecraft:custom_data".to_string(), serde_json::Value::Object(custom_data));
        }
        else if lower.contains("blank_spell_book") && !item.components.contains_key("minecraft:lore") {
            let lore = vec![
                "§7Can bind a spell discovered in the world§r".to_string(),
            ];
            if let Ok(v) = serde_json::to_value(&lore) {
                item.components.insert("minecraft:lore".to_string(), v);
            }
            let mut custom_data = serde_json::Map::new();
            custom_data.insert("blank_spell_book".to_string(), serde_json::json!(true));
            item.components.insert("minecraft:custom_data".to_string(), serde_json::Value::Object(custom_data));
        }
        // 4. Magic Crystals
        else if lower.contains("magic_crystal") && !item.components.contains_key("minecraft:lore") {
            let (elem, elem_col) = infer_element(&lower);
            let lore = vec![
                format!("§9Element: {elem_col}{elem}§r"),
                "§dPure magical energy. Used for crafting and wand upgrades.§r".to_string(),
            ];
            if let Ok(v) = serde_json::to_value(&lore) {
                item.components.insert("minecraft:lore".to_string(), v);
            }
            let mut custom_data = serde_json::Map::new();
            custom_data.insert("mana_crystal".to_string(), serde_json::json!(true));
            item.components.insert("minecraft:custom_data".to_string(), serde_json::Value::Object(custom_data));
        }
    }
    
    for block in &mut ir.blocks {
        let lower = block.id.to_ascii_lowercase();
        if (lower.contains("workbench") || lower.contains("altar") || lower.contains("pedestal"))
            && !block.components.contains_key("minecraft:lore")
        {
            let lore = vec![
                "§7Used to bind spells and upgrade wands§r".to_string(),
                "§8Place magic crystals to upgrade wand tiers§r".to_string(),
            ];
            if let Ok(v) = serde_json::to_value(&lore) {
                block.components.insert("minecraft:lore".to_string(), v);
            }
        }
    }
}

/// Synthesizes authentic declarative ActionEffects for any Electroblob's Wizardry spell.
pub fn synthesize_spell_actions(modid: &str, spell: &str, consume_on_use: bool) -> Vec<ActionEffect> {
    let lower = spell.to_ascii_lowercase();
    let mut effects = Vec::new();

    // 1. Necromancy & Minions
    if lower.contains("zombie_apocalypse") {
        effects.push(ActionEffect::PlaySound {
            sound: "entity.evoker.cast_spell".to_string(),
            volume: 1.0,
            pitch: 0.8,
        });
        effects.push(ActionEffect::PlaySound {
            sound: "entity.zombie.ambient".to_string(),
            volume: 1.2,
            pitch: 0.8,
        });
        effects.push(ActionEffect::SpawnEntity {
            entity_type: "zombie".to_string(),
            count: 6,
        });
        effects.push(ActionEffect::Damage {
            amount: 8.0,
            damage_type: Some("magic".to_string()),
        });
    } else if lower.contains("summon_zombie") {
        effects.push(ActionEffect::PlaySound {
            sound: "entity.zombie.ambient".to_string(),
            volume: 1.0,
            pitch: 1.0,
        });
        effects.push(ActionEffect::SpawnEntity {
            entity_type: "zombie".to_string(),
            count: 1,
        });
    } else if lower.contains("summon_skeleton_legion") {
        effects.push(ActionEffect::PlaySound {
            sound: "entity.skeleton.ambient".to_string(),
            volume: 1.0,
            pitch: 0.9,
        });
        effects.push(ActionEffect::SpawnEntity {
            entity_type: "skeleton".to_string(),
            count: 4,
        });
    } else if lower.contains("summon_skeleton") {
        effects.push(ActionEffect::PlaySound {
            sound: "entity.skeleton.ambient".to_string(),
            volume: 1.0,
            pitch: 1.0,
        });
        effects.push(ActionEffect::SpawnEntity {
            entity_type: "skeleton".to_string(),
            count: 1,
        });
    } else if lower.contains("summon_wither_skeleton") {
        effects.push(ActionEffect::PlaySound {
            sound: "entity.wither_skeleton.ambient".to_string(),
            volume: 1.0,
            pitch: 0.9,
        });
        effects.push(ActionEffect::SpawnEntity {
            entity_type: "wither_skeleton".to_string(),
            count: 1,
        });
    } else if lower.contains("summon_blaze") {
        effects.push(ActionEffect::PlaySound {
            sound: "entity.blaze.ambient".to_string(),
            volume: 1.0,
            pitch: 1.0,
        });
        effects.push(ActionEffect::SpawnEntity {
            entity_type: "blaze".to_string(),
            count: 1,
        });
    } else if lower.contains("summon_snow_golem") {
        effects.push(ActionEffect::PlaySound {
            sound: "entity.snow_golem.ambient".to_string(),
            volume: 1.0,
            pitch: 1.0,
        });
        effects.push(ActionEffect::SpawnEntity {
            entity_type: "snow_golem".to_string(),
            count: 1,
        });
    } else if lower.contains("summon_ice_giant") {
        effects.push(ActionEffect::PlaySound {
            sound: "entity.iron_golem.ambient".to_string(),
            volume: 1.0,
            pitch: 0.8,
        });
        effects.push(ActionEffect::SpawnEntity {
            entity_type: "iron_golem".to_string(),
            count: 1,
        });
    } else if lower.contains("spider_swarm") {
        effects.push(ActionEffect::PlaySound {
            sound: "entity.spider.ambient".to_string(),
            volume: 1.0,
            pitch: 1.2,
        });
        effects.push(ActionEffect::SpawnEntity {
            entity_type: "cave_spider".to_string(),
            count: 4,
        });
    } else if lower.contains("silverfish_swarm") {
        effects.push(ActionEffect::PlaySound {
            sound: "entity.silverfish.ambient".to_string(),
            volume: 1.0,
            pitch: 1.0,
        });
        effects.push(ActionEffect::SpawnEntity {
            entity_type: "silverfish".to_string(),
            count: 5,
        });
    } else if lower.contains("vex_swarm") {
        effects.push(ActionEffect::PlaySound {
            sound: "entity.vex.charge".to_string(),
            volume: 1.0,
            pitch: 1.0,
        });
        effects.push(ActionEffect::SpawnEntity {
            entity_type: "vex".to_string(),
            count: 3,
        });
    } else if lower.contains("slime") {
        effects.push(ActionEffect::PlaySound {
            sound: "entity.slime.squish".to_string(),
            volume: 1.0,
            pitch: 1.0,
        });
        effects.push(ActionEffect::SpawnEntity {
            entity_type: "slime".to_string(),
            count: 2,
        });
    }
    // 2. Pyromancy & Explosions
    else if lower.contains("fireball") || lower.contains("dragon_fireball") {
        effects.push(ActionEffect::PlaySound {
            sound: "entity.blaze.shoot".to_string(),
            volume: 1.0,
            pitch: 1.0,
        });
        effects.push(ActionEffect::SpawnEntity {
            entity_type: "small_fireball".to_string(),
            count: 1,
        });
        effects.push(ActionEffect::Explode {
            power: 3.5,
            causes_fire: true,
            break_blocks: false,
        });
    } else if lower.contains("firebomb") || lower.contains("meteor") || lower.contains("detonate") || lower.contains("combustion_rune") || lower.contains("firestorm") {
        effects.push(ActionEffect::PlaySound {
            sound: "entity.generic.explode".to_string(),
            volume: 1.5,
            pitch: 1.0,
        });
        effects.push(ActionEffect::Explode {
            power: 4.5,
            causes_fire: true,
            break_blocks: true,
        });
    } else if lower.contains("fire_breath") || lower.contains("flame_ray") || lower.contains("firebolt") || lower.contains("ignite") {
        effects.push(ActionEffect::PlaySound {
            sound: "item.firecharge.use".to_string(),
            volume: 1.0,
            pitch: 1.2,
        });
        effects.push(ActionEffect::Damage {
            amount: 8.0,
            damage_type: Some("in_fire".to_string()),
        });
    }
    // 3. Electromancy
    else if lower.contains("lightning_bolt") || lower.contains("thunderbolt") {
        effects.push(ActionEffect::PlaySound {
            sound: "entity.lightning_bolt.thunder".to_string(),
            volume: 1.5,
            pitch: 1.0,
        });
        effects.push(ActionEffect::SpawnEntity {
            entity_type: "lightning_bolt".to_string(),
            count: 1,
        });
        effects.push(ActionEffect::Damage {
            amount: 10.0,
            damage_type: Some("lightning_bolt".to_string()),
        });
    } else if lower.contains("chain_lightning") || lower.contains("arc") || lower.contains("homing_spark") || lower.contains("spark") || lower.contains("lightning_pulse") || lower.contains("lightning_railgun") {
        effects.push(ActionEffect::PlaySound {
            sound: "entity.lightning_bolt.impact".to_string(),
            volume: 1.0,
            pitch: 1.4,
        });
        effects.push(ActionEffect::Damage {
            amount: 9.0,
            damage_type: Some("lightning_bolt".to_string()),
        });
    }
    // 4. Cryomancy
    else if lower.contains("freeze") || lower.contains("blizzard") || lower.contains("hailstorm") || lower.contains("ice_age") || lower.contains("glacial_oblivion") {
        effects.push(ActionEffect::PlaySound {
            sound: "block.powder_snow.break".to_string(),
            volume: 1.0,
            pitch: 0.8,
        });
        effects.push(ActionEffect::ApplyEffect {
            effect: "slowness".to_string(),
            duration_ticks: 240,
            amplifier: 4,
        });
        effects.push(ActionEffect::Damage {
            amount: 12.0,
            damage_type: Some("freeze".to_string()),
        });
    } else if lower.contains("ice_shard") || lower.contains("ice_lance") || lower.contains("ice_ball") || lower.contains("ice_charge") || lower.contains("frost_ray") {
        effects.push(ActionEffect::PlaySound {
            sound: "block.glass.break".to_string(),
            volume: 1.0,
            pitch: 1.5,
        });
        effects.push(ActionEffect::ApplyEffect {
            effect: "slowness".to_string(),
            duration_ticks: 140,
            amplifier: 2,
        });
        effects.push(ActionEffect::Damage {
            amount: 8.0,
            damage_type: Some("freeze".to_string()),
        });
    }
    // 5. Healing & Restoration
    else if lower.contains("heal") || lower.contains("vitality") {
        effects.push(ActionEffect::PlaySound {
            sound: "entity.player.levelup".to_string(),
            volume: 1.0,
            pitch: 1.5,
        });
        effects.push(ActionEffect::Heal { amount: 10.0 });
    } else if lower.contains("cure_effects") || lower.contains("remove_curse") {
        effects.push(ActionEffect::PlaySound {
            sound: "entity.totem.use".to_string(),
            volume: 0.8,
            pitch: 1.5,
        });
        effects.push(ActionEffect::ApplyEffect {
            effect: "regeneration".to_string(),
            duration_ticks: 120,
            amplifier: 1,
        });
    } else if lower.contains("satiety") || lower.contains("replenish_hunger") {
        effects.push(ActionEffect::PlaySound {
            sound: "entity.player.burp".to_string(),
            volume: 1.0,
            pitch: 1.0,
        });
        effects.push(ActionEffect::ApplyEffect {
            effect: "saturation".to_string(),
            duration_ticks: 60,
            amplifier: 1,
        });
    }
    // 6. Sorcery & Teleportation
    else if lower.contains("blink") {
        effects.push(ActionEffect::PlaySound {
            sound: "entity.enderman.teleport".to_string(),
            volume: 1.0,
            pitch: 1.0,
        });
        effects.push(ActionEffect::Teleport {
            offset_x: 0.0,
            offset_y: 0.0,
            offset_z: 8.0,
        });
    } else if lower.contains("phase_step") || lower.contains("teleport") {
        effects.push(ActionEffect::PlaySound {
            sound: "entity.enderman.teleport".to_string(),
            volume: 1.0,
            pitch: 1.2,
        });
        effects.push(ActionEffect::Teleport {
            offset_x: 0.0,
            offset_y: 0.0,
            offset_z: 16.0,
        });
    } else if lower.contains("flight") || lower.contains("better_flight") || lower.contains("levitation") {
        effects.push(ActionEffect::PlaySound {
            sound: "entity.phantom.flap".to_string(),
            volume: 1.0,
            pitch: 1.2,
        });
        effects.push(ActionEffect::ApplyEffect {
            effect: "levitation".to_string(),
            duration_ticks: 160,
            amplifier: 1,
        });
        effects.push(ActionEffect::ApplyEffect {
            effect: "slow_falling".to_string(),
            duration_ticks: 240,
            amplifier: 1,
        });
    } else if lower.contains("agility") {
        effects.push(ActionEffect::PlaySound {
            sound: "entity.horse.gallop".to_string(),
            volume: 0.8,
            pitch: 1.5,
        });
        effects.push(ActionEffect::ApplyEffect {
            effect: "speed".to_string(),
            duration_ticks: 600,
            amplifier: 2,
        });
    } else if lower.contains("invisibility") {
        effects.push(ActionEffect::PlaySound {
            sound: "block.beacon.ambient".to_string(),
            volume: 0.8,
            pitch: 1.8,
        });
        effects.push(ActionEffect::ApplyEffect {
            effect: "invisibility".to_string(),
            duration_ticks: 600,
            amplifier: 1,
        });
    } else if lower.contains("fire_resistance") || lower.contains("fire_skin") {
        effects.push(ActionEffect::PlaySound {
            sound: "item.firecharge.use".to_string(),
            volume: 1.0,
            pitch: 1.0,
        });
        effects.push(ActionEffect::ApplyEffect {
            effect: "fire_resistance".to_string(),
            duration_ticks: 1200,
            amplifier: 1,
        });
    } else if lower.contains("oakflesh") || lower.contains("ironflesh") || lower.contains("diamondflesh") || lower.contains("ward") || lower.contains("forcefield") {
        effects.push(ActionEffect::PlaySound {
            sound: "item.armor.equip_diamond".to_string(),
            volume: 1.0,
            pitch: 1.0,
        });
        effects.push(ActionEffect::ApplyEffect {
            effect: "resistance".to_string(),
            duration_ticks: 600,
            amplifier: 2,
        });
        effects.push(ActionEffect::ApplyEffect {
            effect: "absorption".to_string(),
            duration_ticks: 600,
            amplifier: 2,
        });
    } else if lower.contains("decay") || lower.contains("wither") || lower.contains("curse_of_undeath") || lower.contains("curse_of_enfeeblement") {
        effects.push(ActionEffect::PlaySound {
            sound: "entity.wither.ambient".to_string(),
            volume: 0.8,
            pitch: 1.2,
        });
        effects.push(ActionEffect::ApplyEffect {
            effect: "wither".to_string(),
            duration_ticks: 200,
            amplifier: 1,
        });
        effects.push(ActionEffect::Damage {
            amount: 8.0,
            damage_type: Some("wither".to_string()),
        });
    } else if lower.contains("fangs") {
        effects.push(ActionEffect::PlaySound {
            sound: "entity.evoker_fangs.attack".to_string(),
            volume: 1.0,
            pitch: 1.0,
        });
        effects.push(ActionEffect::SpawnEntity {
            entity_type: "evoker_fangs".to_string(),
            count: 3,
        });
    } else if lower.contains("boulder") || lower.contains("tornado") || lower.contains("whirlwind") || lower.contains("earth_tremor") || lower.contains("tectonic_shatter") {
        effects.push(ActionEffect::PlaySound {
            sound: "entity.ravager.roar".to_string(),
            volume: 1.0,
            pitch: 0.9,
        });
        effects.push(ActionEffect::Explode {
            power: 3.5,
            causes_fire: false,
            break_blocks: false,
        });
    } else {
        // Universal Arcane fallback
        effects.push(ActionEffect::PlaySound {
            sound: format!("{modid}:item.wand.blast"),
            volume: 0.8,
            pitch: 1.2,
        });
        effects.push(ActionEffect::Damage {
            amount: 7.0,
            damage_type: Some("magic".to_string()),
        });
    }

    if consume_on_use {
        effects.push(ActionEffect::ConsumeItem { count: 1 });
    }

    effects
}

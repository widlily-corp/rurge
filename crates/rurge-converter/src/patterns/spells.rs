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

            // Build spell action
            let mut effects = Vec::new();

            // Sound
            effects.push(ActionEffect::PlaySound {
                sound: format!("{modid}:item.wand.chargeup"),
                volume: 1.0,
                pitch: 1.0,
            });

            // Projectile entity or effect or explosion
            let entity_id = format!("{modid}:{spell}");
            let has_entity = ir.entity_types.iter().any(|e| e.id == entity_id);
            let has_effect = ir.mob_effects.iter().any(|e| e.id == entity_id);

            if has_entity {
                effects.push(ActionEffect::SpawnEntity {
                    entity_type: entity_id,
                    count: 1,
                });
            } else if has_effect {
                effects.push(ActionEffect::ApplyEffect {
                    effect: entity_id,
                    duration_ticks: 200,
                    amplifier: 1,
                });
            } else if spell.contains("bomb") || spell.contains("meteor") || spell.contains("detonate") {
                effects.push(ActionEffect::Explode {
                    power: 3.5,
                    causes_fire: false,
                    break_blocks: true,
                });
            } else {
                effects.push(ActionEffect::PlaySound {
                    sound: format!("{modid}:item.wand.blast"),
                    volume: 0.8,
                    pitch: 1.2,
                });
            }

            // Consume scroll on use
            effects.push(ActionEffect::ConsumeItem { count: 1 });

            let action = ItemActionDef::new(ItemTrigger::RightClick, effects)
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
        }
        // 3. Blank Books & Scrolls
        else if lower.contains("blank_scroll") && !item.components.contains_key("minecraft:lore") {
            let lore = vec![
                "§7Can be inscribed with a spell at an Arcane Workbench§r".to_string(),
            ];
            if let Ok(v) = serde_json::to_value(&lore) {
                item.components.insert("minecraft:lore".to_string(), v);
            }
        }
        else if lower.contains("blank_spell_book") && !item.components.contains_key("minecraft:lore") {
            let lore = vec![
                "§7Can bind a spell discovered in the world§r".to_string(),
            ];
            if let Ok(v) = serde_json::to_value(&lore) {
                item.components.insert("minecraft:lore".to_string(), v);
            }
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
        }
    }

    // 5. Blocks (Arcane Workbench, Altar)
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

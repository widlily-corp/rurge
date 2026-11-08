//! Multi-loader pattern matcher and synthesizer for custom mod Spells (such as Electroblob's Wizardry).
//!
//! Synthesizes concrete Spell Book and Scroll items, assigns casting actions, and binds entities/effects.

use crate::bytecode::ClassFile;
use crate::ir::{ActionEffect, ContentIR, ItemActionDef, ItemDef, ItemTrigger};
use crate::manifest::ModMetadata;
use std::collections::BTreeSet;

/// Scans classes and metadata for custom spell definitions and expands them into concrete spell items.
pub fn scan_spells(classes: &[ClassFile], meta: &ModMetadata, ir: &mut ContentIR) {
    let modid = &meta.id;
    let mut spell_ids = BTreeSet::new();

    // 1. Scan classes (like EBSpells or similar spell registries) for spell definitions
    for class in classes {
        let class_name = class.class_name();
        if class_name.contains("Spell") || class_name.contains("spell") {
            // Scan constant pool strings
            for entry in class.constant_pool.entries() {
                if let crate::bytecode::ConstantPoolEntry::Utf8(s) = entry
                    && s.len() >= 3
                    && s.chars().all(|c| c.is_ascii_lowercase() || c == '_')
                    && !s.starts_with("lambda")
                    && !s.starts_with("metafactory")
                    && !s.starts_with("ebwizardry")
                    && !s.starts_with("spell")
                    && !["get", "this", "none", "spells", "register", "init"].contains(&s.as_str())
                {
                    spell_ids.insert(s.clone());
                }
            }
        }
    }

    // 2. Scan lang keys for `spell.<modid>.<name>`
    let spell_prefix = format!("spell.{modid}.");
    for key in ir.lang.keys() {
        if let Some(sub) = key.strip_prefix(&spell_prefix)
            && !sub.contains('.')
            && sub != "conjure_item"
        {
            spell_ids.insert(sub.to_string());
        }
    }

    // If no spells found, exit early
    if spell_ids.is_empty() {
        return;
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

        // 1. Spell Book
        if !ir.items.iter().any(|i| i.id == book_id) {
            let mut book = ItemDef::new(&book_id);
            book.max_stack_size = 1;
            book.rarity = "UNCOMMON".to_string();
            new_items.push(book);
            new_lang.push((format!("item.{modid}.spell_book_{spell}"), format!("Spell Book: {spell_name}")));
        }

        // 2. Scroll
        if !ir.items.iter().any(|i| i.id == scroll_id) {
            let mut scroll = ItemDef::new(&scroll_id);
            scroll.max_stack_size = 16;
            scroll.rarity = "COMMON".to_string();

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
}

//! Multi-loader registry pattern matchers for Fabric, Forge, and NeoForge.

pub mod effects;
pub mod entities;
pub mod fabric;
pub mod forge;
pub mod spells;

use crate::bytecode::{instruction_length, ClassFile, ConstantPool, ConstantPoolEntry};
use crate::ir::ContentIR;
use crate::manifest::{ModLoader, ModMetadata};

/// High-level decoded JVM instruction for pattern matching.
#[derive(Debug, Clone, PartialEq)]
pub enum Opcode {
    LdcString(String),
    LdcInt(i32),
    LdcFloat(f32),
    LdcClass(String),
    ConstInt(i32),
    ConstFloat(f32),
    GetStatic {
        class_name: String,
        name: String,
        descriptor: String,
    },
    PutStatic {
        class_name: String,
        name: String,
        descriptor: String,
    },
    GetField {
        class_name: String,
        name: String,
        descriptor: String,
    },
    PutField {
        class_name: String,
        name: String,
        descriptor: String,
    },
    InvokeVirtual {
        class_name: String,
        name: String,
        descriptor: String,
    },
    InvokeSpecial {
        class_name: String,
        name: String,
        descriptor: String,
    },
    InvokeStatic {
        class_name: String,
        name: String,
        descriptor: String,
    },
    InvokeInterface {
        class_name: String,
        name: String,
        descriptor: String,
    },
    InvokeDynamic {
        bootstrap_idx: u16,
        name: String,
        descriptor: String,
    },
    New(String),
    Other(u8),
}

/// A decoded instruction with its bytecode offset and recognized opcode.
#[derive(Debug, Clone)]
pub struct DecodedInstruction {
    pub offset: usize,
    pub raw_opcode: u8,
    pub op: Opcode,
}

/// Decodes bytecode for a method into a sequence of high-level instructions.
pub fn decode_method_instructions(code: &[u8], cp: &ConstantPool) -> Vec<DecodedInstruction> {
    let mut instructions = Vec::new();
    let mut offset = 0;
    while offset < code.len() {
        let raw = code[offset];
        let len = instruction_length(code, offset);
        let op = match raw {
            // iconst_m1 .. iconst_5
            0x02..=0x08 => Opcode::ConstInt((raw as i32) - 3),
            // fconst_0 .. fconst_2
            0x0B..=0x0D => Opcode::ConstFloat((raw - 0x0B) as f32),
            // bipush
            0x10 if offset + 1 < code.len() => Opcode::ConstInt(code[offset + 1] as i8 as i32),
            // sipush
            0x11 if offset + 2 < code.len() => {
                let val = i16::from_be_bytes([code[offset + 1], code[offset + 2]]);
                Opcode::ConstInt(val as i32)
            }
            // ldc (index u8)
            0x12 if offset + 1 < code.len() => {
                let idx = code[offset + 1] as u16;
                resolve_ldc_entry(idx, cp)
            }
            // ldc_w (index u16)
            0x13 if offset + 2 < code.len() => {
                let idx = u16::from_be_bytes([code[offset + 1], code[offset + 2]]);
                resolve_ldc_entry(idx, cp)
            }
            // getstatic
            0xB2 if offset + 2 < code.len() => {
                let idx = u16::from_be_bytes([code[offset + 1], code[offset + 2]]);
                if let Some((cls, name, desc)) = cp.resolve_field_ref(idx) {
                    Opcode::GetStatic {
                        class_name: cls.to_string(),
                        name: name.to_string(),
                        descriptor: desc.to_string(),
                    }
                } else {
                    Opcode::Other(raw)
                }
            }
            // putstatic
            0xB3 if offset + 2 < code.len() => {
                let idx = u16::from_be_bytes([code[offset + 1], code[offset + 2]]);
                if let Some((cls, name, desc)) = cp.resolve_field_ref(idx) {
                    Opcode::PutStatic {
                        class_name: cls.to_string(),
                        name: name.to_string(),
                        descriptor: desc.to_string(),
                    }
                } else {
                    Opcode::Other(raw)
                }
            }
            // getfield
            0xB4 if offset + 2 < code.len() => {
                let idx = u16::from_be_bytes([code[offset + 1], code[offset + 2]]);
                if let Some((cls, name, desc)) = cp.resolve_field_ref(idx) {
                    Opcode::GetField {
                        class_name: cls.to_string(),
                        name: name.to_string(),
                        descriptor: desc.to_string(),
                    }
                } else {
                    Opcode::Other(raw)
                }
            }
            // putfield
            0xB5 if offset + 2 < code.len() => {
                let idx = u16::from_be_bytes([code[offset + 1], code[offset + 2]]);
                if let Some((cls, name, desc)) = cp.resolve_field_ref(idx) {
                    Opcode::PutField {
                        class_name: cls.to_string(),
                        name: name.to_string(),
                        descriptor: desc.to_string(),
                    }
                } else {
                    Opcode::Other(raw)
                }
            }
            // invokevirtual
            0xB6 if offset + 2 < code.len() => {
                let idx = u16::from_be_bytes([code[offset + 1], code[offset + 2]]);
                if let Some((cls, name, desc)) = cp.resolve_method_ref(idx) {
                    Opcode::InvokeVirtual {
                        class_name: cls.to_string(),
                        name: name.to_string(),
                        descriptor: desc.to_string(),
                    }
                } else {
                    Opcode::Other(raw)
                }
            }
            // invokespecial
            0xB7 if offset + 2 < code.len() => {
                let idx = u16::from_be_bytes([code[offset + 1], code[offset + 2]]);
                if let Some((cls, name, desc)) = cp.resolve_method_ref(idx) {
                    Opcode::InvokeSpecial {
                        class_name: cls.to_string(),
                        name: name.to_string(),
                        descriptor: desc.to_string(),
                    }
                } else {
                    Opcode::Other(raw)
                }
            }
            // invokestatic
            0xB8 if offset + 2 < code.len() => {
                let idx = u16::from_be_bytes([code[offset + 1], code[offset + 2]]);
                if let Some((cls, name, desc)) = cp.resolve_method_ref(idx) {
                    Opcode::InvokeStatic {
                        class_name: cls.to_string(),
                        name: name.to_string(),
                        descriptor: desc.to_string(),
                    }
                } else {
                    Opcode::Other(raw)
                }
            }
            // invokeinterface
            0xB9 if offset + 2 < code.len() => {
                let idx = u16::from_be_bytes([code[offset + 1], code[offset + 2]]);
                if let Some((cls, name, desc)) = cp.resolve_method_ref(idx) {
                    Opcode::InvokeInterface {
                        class_name: cls.to_string(),
                        name: name.to_string(),
                        descriptor: desc.to_string(),
                    }
                } else {
                    Opcode::Other(raw)
                }
            }
            // new
            0xBB if offset + 2 < code.len() => {
                let idx = u16::from_be_bytes([code[offset + 1], code[offset + 2]]);
                if let Some(cls) = cp.get_class_name(idx) {
                    Opcode::New(cls.to_string())
                } else {
                    Opcode::Other(raw)
                }
            }
            _ => Opcode::Other(raw),
        };

        instructions.push(DecodedInstruction {
            offset,
            raw_opcode: raw,
            op,
        });

        offset += if len > 0 { len } else { 1 };
    }
    instructions
}

fn resolve_ldc_entry(index: u16, cp: &ConstantPool) -> Opcode {
    match cp.entry(index) {
        Some(ConstantPoolEntry::StringRef { string_index }) => {
            if let Some(s) = cp.get_utf8(*string_index) {
                Opcode::LdcString(s.to_string())
            } else {
                Opcode::Other(0x12)
            }
        }
        Some(ConstantPoolEntry::Utf8(s)) => Opcode::LdcString(s.clone()),
        Some(ConstantPoolEntry::Integer(v)) => Opcode::LdcInt(*v),
        Some(ConstantPoolEntry::Float(v)) => Opcode::LdcFloat(*v),
        Some(ConstantPoolEntry::Class { name_index }) => {
            if let Some(s) = cp.get_utf8(*name_index) {
                Opcode::LdcClass(s.to_string())
            } else {
                Opcode::Other(0x12)
            }
        }
        _ => Opcode::Other(0x12),
    }
}

/// Scans a collection of parsed JVM classfiles for block and item registrations
/// based on the detected loader in `meta`.
pub fn scan_classes_for_content(classes: &[ClassFile], meta: &ModMetadata) -> ContentIR {
    let mut ir = ContentIR::new(meta.clone());
    match meta.loader {
        ModLoader::Fabric | ModLoader::Quilt => {
            fabric::scan_fabric_classes(classes, meta, &mut ir);
        }
        ModLoader::Forge | ModLoader::NeoForge | ModLoader::LegacyForge => {
            forge::scan_forge_classes(classes, meta, &mut ir);
        }
        ModLoader::Unknown => {
            fabric::scan_fabric_classes(classes, meta, &mut ir);
            forge::scan_forge_classes(classes, meta, &mut ir);
        }
    }
    entities::scan_entities(classes, meta, &mut ir);
    effects::scan_effects(classes, meta, &mut ir);
    spells::scan_spells(classes, meta, &mut ir);

    // Scan classes for item actions and block triggers
    for class in classes {
        let class_name = class.class_name();
        let simple_name = class_name
            .rsplit('/')
            .next()
            .unwrap_or(class_name)
            .to_lowercase();

        // Skip generic class names that cause spurious matches across entire package
        let is_generic = (simple_name.starts_with("eb") && simple_name.ends_with("items"))
            || (simple_name.starts_with("eb") && simple_name.ends_with("blocks"))
            || ["item", "block", "mod", "common", "registry", "registries", "init", "setup", "wizardry"].contains(&simple_name.as_str());

        if !is_generic {
            let class_stem = simple_name.strip_suffix("item").unwrap_or(&simple_name).replace('_', "");
            let mut new_item_actions = Vec::new();
            for item in &mut ir.items {
                let item_sub = item.id.split_once(':').map(|(_, s)| s).unwrap_or(&item.id);
                let clean_sub = item_sub.replace('_', "");
                let is_match = !class_stem.is_empty() && (
                    clean_sub == class_stem
                    || clean_sub.starts_with(&class_stem)
                    || clean_sub.ends_with(&class_stem)
                    || class_stem.starts_with(&clean_sub)
                    || class_stem.ends_with(&clean_sub)
                );
                if is_match {
                    let actions = crate::bytecode::action_extractor::extract_item_actions(class, &item.id);
                    for action in actions {
                        item.actions.push(action.clone());
                        new_item_actions.push(action);
                    }
                }
            }
            for action in new_item_actions {
                ir.add_item_action(action);
            }

            let block_stem = simple_name.strip_suffix("block").unwrap_or(&simple_name).replace('_', "");
            let mut new_block_triggers = Vec::new();
            for block in &mut ir.blocks {
                let block_sub = block.id.split_once(':').map(|(_, s)| s).unwrap_or(&block.id);
                let clean_sub = block_sub.replace('_', "");
                let is_match = !block_stem.is_empty() && (
                    clean_sub == block_stem
                    || clean_sub.starts_with(&block_stem)
                    || clean_sub.ends_with(&block_stem)
                    || block_stem.starts_with(&clean_sub)
                    || block_stem.ends_with(&clean_sub)
                );
                if is_match {
                    let triggers = crate::bytecode::action_extractor::extract_block_triggers(class, &block.id);
                    for trigger in triggers {
                        block.triggers.push(trigger.clone());
                        new_block_triggers.push(trigger);
                    }
                }
            }
            for trigger in new_block_triggers {
                ir.add_block_trigger(trigger);
            }
        }
    }

    // Ensure all wands and mana flasks have rich right-click actions
    let modid = &meta.id;
    let mut synthesized_actions = Vec::new();
    for item in &mut ir.items {
        if item.id.contains("wand") && item.actions.is_empty() {
            let action = crate::ir::ItemActionDef::new(
                crate::ir::ItemTrigger::RightClick,
                vec![
                    crate::ir::ActionEffect::PlaySound {
                        sound: format!("{modid}:item.wand.chargeup"),
                        volume: 1.0,
                        pitch: 1.0,
                    },
                    crate::ir::ActionEffect::SpawnEntity {
                        entity_type: format!("{modid}:magic_missile"),
                        count: 1,
                    },
                    crate::ir::ActionEffect::PlaySound {
                        sound: format!("{modid}:item.wand.blast"),
                        volume: 0.8,
                        pitch: 1.2,
                    },
                ],
            )
            .with_item_id(item.id.clone())
            .with_cooldown(15);
            item.actions.push(action.clone());
            synthesized_actions.push(action);
        } else if item.id.contains("mana_flask") && item.actions.is_empty() {
            let action = crate::ir::ItemActionDef::new(
                crate::ir::ItemTrigger::RightClick,
                vec![
                    crate::ir::ActionEffect::PlaySound {
                        sound: format!("{modid}:item.mana_flask.use"),
                        volume: 1.0,
                        pitch: 1.0,
                    },
                    crate::ir::ActionEffect::ConsumeItem { count: 1 },
                ],
            )
            .with_item_id(item.id.clone())
            .with_cooldown(20);
            item.actions.push(action.clone());
            synthesized_actions.push(action);
        } else if item.id.contains("bomb") && item.actions.is_empty() {
            let action = crate::ir::ItemActionDef::new(
                crate::ir::ItemTrigger::RightClick,
                vec![
                    crate::ir::ActionEffect::PlaySound {
                        sound: "minecraft:entity.splash_potion.throw".to_string(),
                        volume: 1.0,
                        pitch: 1.0,
                    },
                    crate::ir::ActionEffect::Explode {
                        power: 3.5,
                        causes_fire: false,
                        break_blocks: true,
                    },
                    crate::ir::ActionEffect::ConsumeItem { count: 1 },
                ],
            )
            .with_item_id(item.id.clone())
            .with_cooldown(20);
            item.actions.push(action.clone());
            synthesized_actions.push(action);
        }
    }
    for action in synthesized_actions {
        ir.add_item_action(action);
    }

    ir
}

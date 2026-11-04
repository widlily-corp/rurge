//! Pattern matchers for Forge `DeferredRegister` and `RegistryObject` registration.

use super::{DecodedInstruction, Opcode};
use crate::bytecode::ClassFile;
use crate::ir::{BlockDef, ContentIR, FoodStats, ItemDef, ToolStats, ToolType};
use crate::manifest::ModMetadata;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RegistryKind {
    Block,
    Item,
    Other,
}

/// Scans classes for Forge `DeferredRegister` instances and `.register(...)` calls.
pub fn scan_forge_classes(classes: &[ClassFile], meta: &ModMetadata, ir: &mut ContentIR) {
    // Phase 1: Discover all DeferredRegister static fields and their registry types
    let mut registers: HashMap<(String, String), (RegistryKind, String)> = HashMap::new();

    for class in classes {
        let class_name = class.class_name();
        for method in &class.methods {
            if let Some(code_info) = &method.code {
                let instructions =
                    super::decode_method_instructions(&code_info.bytecode, &class.constant_pool);
                discover_deferred_registers(&instructions, class_name, meta, &mut registers);
            }
        }
    }

    // Phase 2: Scan for .register(...) calls on DeferredRegister instances
    for class in classes {
        let class_name = class.class_name();
        for method in &class.methods {
            if let Some(code_info) = &method.code {
                let instructions =
                    super::decode_method_instructions(&code_info.bytecode, &class.constant_pool);
                scan_forge_method_registrations(
                    &instructions,
                    class,
                    class_name,
                    meta,
                    &registers,
                    ir,
                );
            }
        }
    }
}

fn discover_deferred_registers(
    instructions: &[DecodedInstruction],
    enclosing_class: &str,
    meta: &ModMetadata,
    registers: &mut HashMap<(String, String), (RegistryKind, String)>,
) {
    for (i, instr) in instructions.iter().enumerate() {
        let is_deferred_create = match &instr.op {
            Opcode::InvokeStatic {
                class_name, name, ..
            } => {
                class_name.contains("DeferredRegister")
                    && (name == "create" || name == "createBlocks" || name == "createItems")
            }
            _ => false,
        };

        if !is_deferred_create {
            continue;
        }

        let mut kind = RegistryKind::Other;
        let mut modid = meta.id.clone();

        if let Opcode::InvokeStatic { name, .. } = &instr.op {
            if name == "createBlocks" {
                kind = RegistryKind::Block;
            } else if name == "createItems" {
                kind = RegistryKind::Item;
            }
        }

        // Look backwards for registry reference and modid
        for prev in instructions[..i].iter().rev() {
            match &prev.op {
                Opcode::LdcString(s) => {
                    if !s.is_empty() && !s.contains('/') {
                        modid = s.clone();
                    }
                }
                Opcode::GetStatic {
                    name: f_name,
                    descriptor,
                    ..
                } => {
                    if f_name == "BLOCKS" || descriptor.contains("Block") {
                        kind = RegistryKind::Block;
                    } else if f_name == "ITEMS" || descriptor.contains("Item") {
                        kind = RegistryKind::Item;
                    }
                }
                _ => {}
            }
        }

        // Look forwards for putstatic to store the DeferredRegister field
        for next in instructions[i + 1..].iter() {
            match &next.op {
                Opcode::PutStatic {
                    class_name: f_class,
                    name: f_name,
                    ..
                } => {
                    registers.insert((f_class.clone(), f_name.clone()), (kind, modid.clone()));
                    break;
                }
                Opcode::InvokeStatic { .. } | Opcode::InvokeVirtual { .. } => break,
                _ => {}
            }
        }

        // Fallback register entry if putstatic was immediate or implicit
        registers.insert((enclosing_class.to_string(), "BLOCKS".to_string()), (RegistryKind::Block, modid.clone()));
        registers.insert((enclosing_class.to_string(), "ITEMS".to_string()), (RegistryKind::Item, modid));
    }
}

fn scan_forge_method_registrations(
    instructions: &[DecodedInstruction],
    class: &ClassFile,
    enclosing_class: &str,
    meta: &ModMetadata,
    registers: &HashMap<(String, String), (RegistryKind, String)>,
    ir: &mut ContentIR,
) {
    let mut reg_call_indices = Vec::new();

    for (i, instr) in instructions.iter().enumerate() {
        if is_forge_register_call(instr) {
            reg_call_indices.push(i);
        }
    }

    let mut last_split = 0;

    for (lambda_counter, &call_idx) in reg_call_indices.iter().enumerate() {
        let window = &instructions[last_split..=call_idx];
        process_forge_registration_window(
            window,
            class,
            enclosing_class,
            meta,
            registers,
            lambda_counter,
            ir,
        );
        last_split = call_idx + 1;
    }
}

fn is_forge_register_call(instr: &DecodedInstruction) -> bool {
    match &instr.op {
        Opcode::InvokeVirtual {
            class_name, name, ..
        }
        | Opcode::InvokeInterface {
            class_name, name, ..
        } => {
            name == "register"
                && (class_name.contains("DeferredRegister")
                    || class_name.contains("Registration")
                    || class_name.contains("RegisterEvent")
                    || class_name.contains("IForgeRegistry"))
        }
        _ => false,
    }
}

fn process_forge_registration_window(
    window: &[DecodedInstruction],
    class: &ClassFile,
    enclosing_class: &str,
    meta: &ModMetadata,
    registers: &HashMap<(String, String), (RegistryKind, String)>,
    lambda_index: usize,
    ir: &mut ContentIR,
) {
    // 1. Determine target registry and modid
    let mut kind = RegistryKind::Other;
    let mut modid = meta.id.clone();
    let mut entry_name = String::new();

    for instr in window.iter().rev() {
        match &instr.op {
            Opcode::GetStatic {
                class_name, name, ..
            } => {
                if let Some((k, mid)) = registers.get(&(class_name.clone(), name.clone())) {
                    kind = *k;
                    modid = mid.clone();
                } else if let Some((k, mid)) = registers.get(&(enclosing_class.to_string(), name.clone())) {
                    kind = *k;
                    modid = mid.clone();
                } else if name.contains("BLOCK") {
                    kind = RegistryKind::Block;
                } else if name.contains("ITEM") {
                    kind = RegistryKind::Item;
                }
            }
            Opcode::LdcString(s)
                if entry_name.is_empty()
                    && !s.is_empty()
                    && !s.contains('/')
                    && !s.starts_with('L') =>
            {
                entry_name = s.clone();
            }
            _ => {}
        }
    }

    if entry_name.is_empty() {
        return;
    }

    let full_id = if entry_name.contains(':') {
        entry_name.clone()
    } else {
        format!("{modid}:{entry_name}")
    };

    // If kind is still Other, infer from entry name
    if kind == RegistryKind::Other {
        let lower = entry_name.to_lowercase();
        if lower.contains("block") || lower.contains("ore") || lower.contains("furnace") {
            kind = RegistryKind::Block;
        } else {
            kind = RegistryKind::Item;
        }
    }

    // 2. Scan properties in window AND in supplier lambda methods
    let mut lambda_instructions = Vec::new();
    find_supplier_instructions(class, lambda_index, &mut lambda_instructions);

    if kind == RegistryKind::Block {
        let mut block = BlockDef::new(&full_id);
        extract_forge_block_properties(window, &mut block);
        if !lambda_instructions.is_empty() {
            extract_forge_block_properties(&lambda_instructions, &mut block);
        }
        ir.add_block(block);
    } else {
        let mut item = ItemDef::new(&full_id);
        extract_forge_item_properties(window, &full_id, &mut item);
        if !lambda_instructions.is_empty() {
            extract_forge_item_properties(&lambda_instructions, &full_id, &mut item);
        }
        ir.add_item(item);
    }
}

fn find_supplier_instructions(
    class: &ClassFile,
    lambda_index: usize,
    output: &mut Vec<DecodedInstruction>,
) {
    let lambda_methods: Vec<_> = class
        .methods
        .iter()
        .filter(|m| m.name.starts_with("lambda$") || m.name.contains("$lambda$"))
        .collect();

    let target_method = lambda_methods.get(lambda_index).or_else(|| lambda_methods.first());

    if let Some(code_info) = target_method.and_then(|m| m.code.as_ref()) {
        *output = super::decode_method_instructions(&code_info.bytecode, &class.constant_pool);
    }
}

fn extract_forge_block_properties(instructions: &[DecodedInstruction], block: &mut BlockDef) {
    for (i, instr) in instructions.iter().enumerate() {
        match &instr.op {
            Opcode::InvokeVirtual { name, .. } | Opcode::InvokeStatic { name, .. } => {
                let lower = name.to_lowercase();
                if lower == "strength" || lower == "destroytime" {
                    let mut floats = Vec::new();
                    for prev in instructions[..i].iter().rev() {
                        match &prev.op {
                            Opcode::LdcFloat(f) => floats.push(*f),
                            Opcode::ConstFloat(f) => floats.push(*f),
                            Opcode::LdcInt(n) => floats.push(*n as f32),
                            Opcode::ConstInt(n) => floats.push(*n as f32),
                            Opcode::InvokeVirtual { .. }
                            | Opcode::InvokeStatic { .. }
                            | Opcode::New(_) => break,
                            _ => {}
                        }
                        if floats.len() >= 2 {
                            break;
                        }
                    }
                    floats.reverse();
                    if floats.len() >= 2 {
                        block.hardness = floats[0];
                        block.resistance = floats[1];
                    } else if floats.len() == 1 {
                        block.hardness = floats[0];
                        block.resistance = floats[0];
                    }
                } else if lower == "lightlevel" || lower == "luminance" {
                    for prev in instructions[..i].iter().rev() {
                        match &prev.op {
                            Opcode::ConstInt(n) | Opcode::LdcInt(n) => {
                                block.light_emission = (*n).clamp(0, 15) as u32;
                                break;
                            }
                            _ => {}
                        }
                    }
                } else if lower == "noocclusion" || lower == "nonopaque" {
                    block.opaque = false;
                } else if lower.contains("requirestool") || lower.contains("requirescorrecttool") {
                    block.tool = Some(ToolType::Pickaxe);
                }
            }
            _ => {}
        }
    }
}

fn extract_forge_item_properties(instructions: &[DecodedInstruction], id: &str, item: &mut ItemDef) {
    for instr in instructions {
        let tool_from_class = match &instr.op {
            Opcode::New(cls) => ToolType::from_name(cls),
            _ => None,
        };
        if let Some(tool_type) = tool_from_class {
            item.tool = Some(ToolStats {
                tool_type,
                speed: 6.0,
                attack_damage: match tool_type {
                    ToolType::Pickaxe => 2.0,
                    ToolType::Axe => 6.0,
                    ToolType::Shovel => 1.5,
                    ToolType::Hoe => 0.0,
                    ToolType::Sword => 4.0,
                },
                mining_level: 2,
            });
            break;
        }
    }

    if item.tool.is_none() {
        item.tool = ToolType::from_name(id).map(|tool_type| ToolStats {
            tool_type,
            speed: 6.0,
            attack_damage: match tool_type {
                ToolType::Pickaxe => 2.0,
                ToolType::Axe => 6.0,
                ToolType::Shovel => 1.5,
                ToolType::Hoe => 0.0,
                ToolType::Sword => 4.0,
            },
            mining_level: 2,
        });
    }

    for (i, instr) in instructions.iter().enumerate() {
        match &instr.op {
            Opcode::InvokeVirtual { name, .. } | Opcode::InvokeStatic { name, .. } => {
                let lower = name.to_lowercase();
                if lower == "stacksto" || lower == "maxcount" {
                    for prev in instructions[..i].iter().rev() {
                        match &prev.op {
                            Opcode::ConstInt(n) | Opcode::LdcInt(n) if *n > 0 => {
                                item.max_stack_size = *n as u32;
                                break;
                            }
                            _ => {}
                        }
                    }
                } else if lower == "durability" || lower == "maxdamage" {
                    for prev in instructions[..i].iter().rev() {
                        match &prev.op {
                            Opcode::ConstInt(n) | Opcode::LdcInt(n) if *n > 0 => {
                                item.durability = *n as u32;
                                break;
                            }
                            _ => {}
                        }
                    }
                } else if lower == "food" {
                    item.food = Some(FoodStats {
                        nutrition: 4,
                        saturation_modifier: 0.6,
                        always_edible: false,
                    });
                }
            }
            _ => {}
        }
    }
}

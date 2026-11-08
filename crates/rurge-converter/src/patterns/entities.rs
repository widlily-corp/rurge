//! Multi-loader pattern matchers for Minecraft `EntityType` registration.
//!
//! Supports Forge/NeoForge `DeferredRegister<EntityType<?>>`, Fabric `Registry.register(Registries.ENTITY_TYPE, ...)`,
//! and builder-level `EntityType.Builder` patterns.

use super::{DecodedInstruction, Opcode};
use crate::bytecode::ClassFile;
use crate::ir::{ContentIR, EntityTypeDef};
use crate::manifest::ModMetadata;
use std::collections::HashMap;

/// Determines template classification for an entity based on its identifier or class name.
pub fn determine_entity_template(name: &str, class_name: Option<&str>) -> &'static str {
    let mut combined = name.to_lowercase();
    if let Some(cls) = class_name {
        combined.push(' ');
        combined.push_str(&cls.to_lowercase());
    }

    if combined.contains("missile")
        || combined.contains("ball")
        || combined.contains("bullet")
        || combined.contains("bolt")
        || combined.contains("shard")
        || combined.contains("arrow")
        || combined.contains("projectile")
    {
        "projectile"
    } else if combined.contains("bomb") || combined.contains("tnt") {
        "tnt"
    } else if combined.contains("item") {
        "item"
    } else if combined.contains("interaction") {
        "interaction"
    } else {
        "living"
    }
}

/// Scans classes for Forge and Fabric entity registrations and populates `ir.entity_types`.
pub fn scan_entities(classes: &[ClassFile], meta: &ModMetadata, ir: &mut ContentIR) {
    // Phase 1: Discover DeferredRegister instances dedicated to EntityType
    let mut registers: HashMap<(String, String), String> = HashMap::new();

    for class in classes {
        let class_name = class.class_name();
        for method in &class.methods {
            if let Some(code_info) = &method.code {
                let instructions =
                    super::decode_method_instructions(&code_info.bytecode, &class.constant_pool);
                discover_entity_registers(&instructions, class_name, meta, &mut registers);
            }
        }
    }

    // Phase 2: Scan for entity registrations in methods
    for class in classes {
        let class_name = class.class_name();
        for method in &class.methods {
            if let Some(code_info) = &method.code {
                let instructions =
                    super::decode_method_instructions(&code_info.bytecode, &class.constant_pool);
                scan_method_entity_registrations(
                    &instructions,
                    class_name,
                    meta,
                    &registers,
                    ir,
                );
            }
        }
    }

    // Phase 3: Scan lambda/builder methods for dimension and tracking refinements
    for class in classes {
        for method in &class.methods {
            if let Some(code_info) = &method.code {
                let instructions =
                    super::decode_method_instructions(&code_info.bytecode, &class.constant_pool);
                refine_entities_from_builders(&instructions, meta, ir);
            }
        }
    }
}

/// Alias for `scan_entities`.
pub use scan_entities as scan_entity_classes;

fn discover_entity_registers(
    instructions: &[DecodedInstruction],
    enclosing_class: &str,
    meta: &ModMetadata,
    registers: &mut HashMap<(String, String), String>,
) {
    for (i, instr) in instructions.iter().enumerate() {
        let is_deferred_create = match &instr.op {
            Opcode::InvokeStatic {
                class_name, name, ..
            } => {
                class_name.contains("DeferredRegister")
                    && (name == "create" || name == "createEntities")
            }
            _ => false,
        };

        if !is_deferred_create {
            continue;
        }

        let mut is_entity = false;
        let mut modid = meta.id.clone();

        // Check backwards for Registries.ENTITY_TYPE or ForgeRegistries.ENTITIES / ENTITY_TYPES
        for prev in instructions[..i].iter().rev() {
            match &prev.op {
                Opcode::LdcString(s) => {
                    if !s.is_empty() && !s.contains('/') {
                        modid = s.clone();
                    }
                }
                Opcode::GetStatic {
                    name,
                    descriptor,
                    class_name,
                } if name == "ENTITIES"
                    || name == "ENTITY_TYPES"
                    || name == "ENTITY_TYPE"
                    || descriptor.contains("EntityType")
                    || class_name.contains("EntityType") =>
                {
                    is_entity = true;
                }
                _ => {}
            }
        }

        if !is_entity {
            continue;
        }

        // Look forwards for putstatic to store the DeferredRegister field
        for next in instructions[i + 1..].iter() {
            match &next.op {
                Opcode::PutStatic {
                    class_name: f_class,
                    name: f_name,
                    ..
                } => {
                    registers.insert((f_class.clone(), f_name.clone()), modid.clone());
                    break;
                }
                Opcode::InvokeStatic { .. } | Opcode::InvokeVirtual { .. } => break,
                _ => {}
            }
        }

        registers.insert((enclosing_class.to_string(), "ENTITIES".to_string()), modid.clone());
        registers.insert((enclosing_class.to_string(), "ENTITY_TYPES".to_string()), modid);
    }
}

fn scan_method_entity_registrations(
    instructions: &[DecodedInstruction],
    enclosing_class: &str,
    meta: &ModMetadata,
    registers: &HashMap<(String, String), String>,
    ir: &mut ContentIR,
) {
    let mut last_split = 0;

    for (i, instr) in instructions.iter().enumerate() {
        if is_entity_register_call(instr, enclosing_class, registers, instructions, i) {
            let window = &instructions[last_split..=i];
            process_entity_registration_window(window, meta, registers, ir);
            last_split = i + 1;
        } else if matches!(&instr.op, Opcode::InvokeStatic { class_name, name, .. } if class_name.contains("DeferredRegister") && (name == "create" || name == "createEntities")) {
            last_split = i + 1;
        }
    }
}

fn is_entity_register_call(
    instr: &DecodedInstruction,
    enclosing_class: &str,
    registers: &HashMap<(String, String), String>,
    all_instructions: &[DecodedInstruction],
    current_idx: usize,
) -> bool {
    match &instr.op {
        Opcode::InvokeVirtual {
            class_name, name, ..
        }
        | Opcode::InvokeInterface {
            class_name, name, ..
        } => {
            if name == "register" {
                if class_name.contains("DeferredRegister") {
                    // Check if instructions leading up to this reference an entity register
                    let window_start = current_idx.saturating_sub(15);
                    for prev in &all_instructions[window_start..current_idx] {
                        if matches!(&prev.op, Opcode::GetStatic { class_name: f_class, name: f_name, .. } if registers.contains_key(&(f_class.clone(), f_name.clone())) || f_name == "ENTITIES" || f_name == "ENTITY_TYPES") {
                            return true;
                        }
                    }
                    return false;
                }
            } else if (name == "build" || name == "create")
                && (class_name.contains("EntityType$Builder")
                    || class_name.contains("EntityTypeBuilder")
                    || class_name.contains("FabricEntityTypeBuilder"))
            {
                return true;
            }
            false
        }
        Opcode::InvokeStatic {
            class_name,
            name,
            descriptor,
        } => {
            // Fabric Registry.register(Registries.ENTITY_TYPE, ...)
            if name == "register"
                && (class_name.contains("registry/Registry")
                    || class_name.contains("registry/Registries")
                    || class_name.ends_with("/Registry")
                    || class_name.ends_with("/Registries"))
            {
                let window_start = current_idx.saturating_sub(15);
                for prev in &all_instructions[window_start..current_idx] {
                    if matches!(&prev.op, Opcode::GetStatic { name: f_name, descriptor: f_desc, .. } if f_name == "ENTITY_TYPE" || f_name == "ENTITIES" || f_desc.contains("EntityType")) {
                        return true;
                    }
                }
                return false;
            }

            // Custom static helper register("entity_name", ...)
            if (name == "register" || name == "registerEntity")
                && (descriptor.contains("EntityType") || class_name == enclosing_class)
            {
                let window_start = current_idx.saturating_sub(10);
                for prev in &all_instructions[window_start..current_idx] {
                    if let Opcode::LdcString(_) = &prev.op {
                        return true;
                    }
                }
            }
            false
        }
        _ => false,
    }
}

fn process_entity_registration_window(
    window: &[DecodedInstruction],
    meta: &ModMetadata,
    registers: &HashMap<(String, String), String>,
    ir: &mut ContentIR,
) {
    let mut entity_name = None;
    let mut entity_class = None;
    let mut custom_width = None;
    let mut custom_height = None;
    let mut custom_tracking_range = None;
    let mut modid = meta.id.clone();

    // First pass: scan backwards from the registration call for entry name and modid
    for instr in window.iter().rev() {
        match &instr.op {
            Opcode::GetStatic {
                class_name, name, ..
            } => {
                if let Some(discovered_modid) = registers.get(&(class_name.clone(), name.clone())) {
                    modid = discovered_modid.clone();
                }
            }
            Opcode::LdcString(s)
                if entity_name.is_none()
                    && ((!s.is_empty() && !s.contains('/') && !s.contains(' '))
                        || s.contains(':')) =>
            {
                entity_name = Some(s.clone());
            }
            _ => {}
        }
    }

    // Scan forward for dimensions and classes
    for (i, instr) in window.iter().enumerate() {
        match &instr.op {
            Opcode::LdcClass(cls) | Opcode::New(cls) => {
                if !cls.contains("EntityType")
                    && !cls.contains("Builder")
                    && !cls.contains("Identifier")
                    && !cls.contains("ResourceLocation")
                    && !cls.contains("Supplier")
                    && entity_class.is_none()
                {
                    entity_class = Some(cls.clone());
                }
            }
            Opcode::InvokeSpecial {
                class_name,
                name,
                ..
            } => {
                if name == "<init>"
                    && !class_name.contains("EntityType")
                    && !class_name.contains("Builder")
                    && !class_name.contains("Identifier")
                    && !class_name.contains("ResourceLocation")
                    && !class_name.contains("Supplier")
                    && entity_class.is_none()
                {
                    entity_class = Some(class_name.clone());
                }
            }
            Opcode::InvokeVirtual { name, descriptor, .. }
            | Opcode::InvokeStatic { name, descriptor, .. } => {
                // Dimensions: sized(FF) or dimensions(FF)
                if (name == "sized" || name == "dimensions") || descriptor.contains("(FF)") {
                    let mut floats = Vec::new();
                    for prev in window[..i].iter().rev() {
                        match &prev.op {
                            Opcode::LdcFloat(f) | Opcode::ConstFloat(f) => floats.push(*f),
                            Opcode::LdcInt(n) | Opcode::ConstInt(n) => floats.push(*n as f32),
                            Opcode::InvokeVirtual { .. }
                            | Opcode::InvokeStatic { .. }
                            | Opcode::InvokeSpecial { .. } => break,
                            _ => {}
                        }
                        if floats.len() == 2 {
                            break;
                        }
                    }
                    if floats.len() == 2 {
                        // First pushed was width, second was height (floats are reversed)
                        custom_height = Some(floats[0]);
                        custom_width = Some(floats[1]);
                    }
                }

                // Tracking range: clientTrackingRange(I) or trackingRange(I) or trackRangeBlocks(I)
                if name.contains("TrackingRange")
                    || name.contains("trackingRange")
                    || name.contains("trackRange")
                {
                    for prev in window[..i].iter().rev() {
                        match &prev.op {
                            Opcode::LdcInt(n) | Opcode::ConstInt(n) if *n > 0 => {
                                custom_tracking_range = Some(*n as u32);
                                break;
                            }
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }
    }

    let name = match entity_name {
        Some(n) => n,
        None => return,
    };

    let full_id = if name.contains(':') {
        name.clone()
    } else {
        format!("{}:{}", modid, name)
    };

    let template = determine_entity_template(&name, entity_class.as_deref());
    let mut def = EntityTypeDef::new(full_id, template);

    if let (Some(w), Some(h)) = (custom_width, custom_height) {
        def.width = w;
        def.height = h;
    }
    if let Some(tr) = custom_tracking_range {
        def.tracking_range = tr;
    }

    ir.add_entity_type(def);
}

fn refine_entities_from_builders(
    instructions: &[DecodedInstruction],
    meta: &ModMetadata,
    ir: &mut ContentIR,
) {
    for (i, instr) in instructions.iter().enumerate() {
        let is_build_call = match &instr.op {
            Opcode::InvokeVirtual { class_name, name, .. } => {
                name == "build"
                    && (class_name.contains("EntityType$Builder")
                        || class_name.contains("EntityTypeBuilder")
                        || class_name.contains("FabricEntityTypeBuilder"))
            }
            _ => false,
        };

        if !is_build_call {
            continue;
        }

        // Look backwards for entity id string or dimensions
        let mut entity_name = None;
        let mut custom_width = None;
        let mut custom_height = None;
        let mut custom_tracking_range = None;
        let mut entity_class = None;

        let window_start = i.saturating_sub(20);
        let window = &instructions[window_start..=i];

        for (w_idx, w_instr) in window.iter().enumerate() {
            match &w_instr.op {
                Opcode::LdcString(s) => {
                    if !s.is_empty() && !s.contains('/') && entity_name.is_none() {
                        entity_name = Some(s.clone());
                    }
                }
                Opcode::LdcClass(cls) | Opcode::New(cls) => {
                    if !cls.contains("EntityType")
                        && !cls.contains("Builder")
                        && entity_class.is_none()
                    {
                        entity_class = Some(cls.clone());
                    }
                }
                Opcode::InvokeVirtual { name, descriptor, .. } => {
                    if (name == "sized" || name == "dimensions") || descriptor.contains("(FF)") {
                        let mut floats = Vec::new();
                        for prev in window[..w_idx].iter().rev() {
                            match &prev.op {
                                Opcode::LdcFloat(f) | Opcode::ConstFloat(f) => floats.push(*f),
                                Opcode::LdcInt(n) | Opcode::ConstInt(n) => floats.push(*n as f32),
                                _ => {}
                            }
                            if floats.len() == 2 {
                                break;
                            }
                        }
                        if floats.len() == 2 {
                            custom_height = Some(floats[0]);
                            custom_width = Some(floats[1]);
                        }
                    }

                    if name.contains("TrackingRange")
                        || name.contains("trackingRange")
                        || name.contains("trackRange")
                    {
                        for prev in window[..w_idx].iter().rev() {
                            match &prev.op {
                                Opcode::LdcInt(n) | Opcode::ConstInt(n) if *n > 0 => {
                                    custom_tracking_range = Some(*n as u32);
                                    break;
                                }
                                _ => {}
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        if let Some(name) = entity_name {
            let full_id = if name.contains(':') {
                name.clone()
            } else {
                format!("{}:{}", meta.id, name)
            };

            if let Some(existing) = ir.entity_types.iter_mut().find(|e| e.id == full_id) {
                if let (Some(w), Some(h)) = (custom_width, custom_height) {
                    existing.width = w;
                    existing.height = h;
                }
                if let Some(tr) = custom_tracking_range {
                    existing.tracking_range = tr;
                }
            } else {
                let template = determine_entity_template(&name, entity_class.as_deref());
                let mut def = EntityTypeDef::new(full_id, template);
                if let (Some(w), Some(h)) = (custom_width, custom_height) {
                    def.width = w;
                    def.height = h;
                }
                if let Some(tr) = custom_tracking_range {
                    def.tracking_range = tr;
                }
                ir.add_entity_type(def);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bytecode::{
        AttributeInfo, ClassFile, CodeInfo, ConstantPool, ConstantPoolEntry, MethodInfo,
    };
    use crate::manifest::ModLoader;

    fn mock_meta() -> ModMetadata {
        ModMetadata {
            id: "ebwizardry".to_string(),
            name: "Electroblob's Wizardry".to_string(),
            version: "4.3.9".to_string(),
            description: "Spellcasting and wizardry".to_string(),
            authors: vec!["Electroblob".to_string()],
            loader: ModLoader::Forge,
        }
    }

    fn push_utf8(cp: &mut ConstantPool, s: &str) -> u16 {
        cp.push(ConstantPoolEntry::Utf8(s.to_string()));
        (cp.len() - 1) as u16
    }

    fn push_string(cp: &mut ConstantPool, s: &str) -> u16 {
        let utf8_idx = push_utf8(cp, s);
        cp.push(ConstantPoolEntry::StringRef {
            string_index: utf8_idx,
        });
        (cp.len() - 1) as u16
    }

    fn push_class(cp: &mut ConstantPool, name: &str) -> u16 {
        let utf8_idx = push_utf8(cp, name);
        cp.push(ConstantPoolEntry::Class {
            name_index: utf8_idx,
        });
        (cp.len() - 1) as u16
    }

    fn push_name_and_type(cp: &mut ConstantPool, name: &str, desc: &str) -> u16 {
        let n_idx = push_utf8(cp, name);
        let d_idx = push_utf8(cp, desc);
        cp.push(ConstantPoolEntry::NameAndType {
            name_index: n_idx,
            descriptor_index: d_idx,
        });
        (cp.len() - 1) as u16
    }

    fn push_method_ref(cp: &mut ConstantPool, class_idx: u16, nat_idx: u16) -> u16 {
        cp.push(ConstantPoolEntry::Methodref {
            class_index: class_idx,
            name_and_type_index: nat_idx,
        });
        (cp.len() - 1) as u16
    }

    fn push_field_ref(cp: &mut ConstantPool, class_idx: u16, nat_idx: u16) -> u16 {
        cp.push(ConstantPoolEntry::Fieldref {
            class_index: class_idx,
            name_and_type_index: nat_idx,
        });
        (cp.len() - 1) as u16
    }

    #[test]
    fn test_determine_entity_template_patterns() {
        assert_eq!(determine_entity_template("magic_missile", None), "projectile");
        assert_eq!(determine_entity_template("fireball", None), "projectile");
        assert_eq!(determine_entity_template("lightning_bolt", None), "projectile");
        assert_eq!(determine_entity_template("ice_shard", None), "projectile");
        assert_eq!(determine_entity_template("magic_bomb", None), "tnt");
        assert_eq!(determine_entity_template("wizard_tnt", None), "tnt");
        assert_eq!(determine_entity_template("spell_book_item", None), "item");
        assert_eq!(determine_entity_template("wizard_interaction", None), "interaction");
        assert_eq!(determine_entity_template("evil_wizard", None), "living");
        assert_eq!(
            determine_entity_template("minion", Some("com/example/entity/ArrowMinion")),
            "projectile"
        );
    }

    #[test]
    fn test_scan_forge_entity_registration() {
        let mut cp = ConstantPool::new();
        let utf_clinit = push_utf8(&mut cp, "<clinit>");
        let utf_void = push_utf8(&mut cp, "()V");
        let utf_code = push_utf8(&mut cp, "Code");

        let cls_def_reg = push_class(&mut cp, "net/minecraftforge/registrations/DeferredRegister");
        let nat_create = push_name_and_type(
            &mut cp,
            "create",
            "(Lnet/minecraft/resources/ResourceKey;Ljava/lang/String;)Lnet/minecraftforge/registrations/DeferredRegister;",
        );
        let mref_create = push_method_ref(&mut cp, cls_def_reg, nat_create);

        let cls_forge_reg = push_class(&mut cp, "net/minecraftforge/registries/ForgeRegistries");
        let nat_entities = push_name_and_type(
            &mut cp,
            "ENTITY_TYPES",
            "Lnet/minecraftforge/registries/IForgeRegistry;",
        );
        let fref_entities = push_field_ref(&mut cp, cls_forge_reg, nat_entities);

        let str_modid = push_string(&mut cp, "ebwizardry");

        let cls_mod = push_class(&mut cp, "com/example/ModEntities");
        let nat_reg_field = push_name_and_type(
            &mut cp,
            "ENTITIES",
            "Lnet/minecraftforge/registrations/DeferredRegister;",
        );
        let fref_reg_field = push_field_ref(&mut cp, cls_mod, nat_reg_field);

        let str_missile = push_string(&mut cp, "magic_missile");
        let nat_register = push_name_and_type(
            &mut cp,
            "register",
            "(Ljava/lang/String;Ljava/util/function/Supplier;)Lnet/minecraftforge/registrations/RegistryObject;",
        );
        let mref_register = push_method_ref(&mut cp, cls_def_reg, nat_register);

        // Bytecode for DeferredRegister.create and ENTITIES.register("magic_missile", ...)
        let mut bytecode = Vec::new();
        // getstatic ForgeRegistries.ENTITY_TYPES
        bytecode.extend_from_slice(&[0xB2, (fref_entities >> 8) as u8, (fref_entities & 0xFF) as u8]);
        // ldc str_modid
        bytecode.extend_from_slice(&[0x12, str_modid as u8]);
        // invokestatic DeferredRegister.create
        bytecode.extend_from_slice(&[0xB8, (mref_create >> 8) as u8, (mref_create & 0xFF) as u8]);
        // putstatic ModEntities.ENTITIES
        bytecode.extend_from_slice(&[0xB3, (fref_reg_field >> 8) as u8, (fref_reg_field & 0xFF) as u8]);

        // getstatic ModEntities.ENTITIES
        bytecode.extend_from_slice(&[0xB2, (fref_reg_field >> 8) as u8, (fref_reg_field & 0xFF) as u8]);
        // ldc str_missile
        bytecode.extend_from_slice(&[0x12, str_missile as u8]);
        // aconst_null
        bytecode.push(0x01);
        // invokevirtual DeferredRegister.register
        bytecode.extend_from_slice(&[0xB6, (mref_register >> 8) as u8, (mref_register & 0xFF) as u8]);
        // return
        bytecode.push(0xB1);

        let method = MethodInfo {
            access_flags: 0x0008,
            name_index: utf_clinit,
            descriptor_index: utf_void,
            name: "<clinit>".to_string(),
            descriptor: "()V".to_string(),
            attributes: vec![AttributeInfo {
                name_index: utf_code,
                name: "Code".to_string(),
                data: Vec::new(),
            }],
            code: Some(CodeInfo {
                max_stack: 4,
                max_locals: 0,
                bytecode,
            }),
        };

        let class_file = ClassFile {
            minor_version: 0,
            major_version: 61,
            constant_pool: cp,
            access_flags: 0x0001,
            this_class: cls_mod,
            super_class: 0,
            interfaces: Vec::new(),
            fields: Vec::new(),
            methods: vec![method],
            attributes: Vec::new(),
        };

        let meta = mock_meta();
        let mut ir = ContentIR::new(meta.clone());
        scan_entity_classes(&[class_file], &meta, &mut ir);

        assert_eq!(ir.entity_types.len(), 1);
        let missile = &ir.entity_types[0];
        assert_eq!(missile.id, "ebwizardry:magic_missile");
        assert_eq!(missile.template, "projectile");
        assert_eq!(missile.width, 0.5);
        assert_eq!(missile.height, 0.5);
        assert_eq!(missile.tracking_range, 64);
    }

    #[test]
    fn test_scan_entity_builder_sized_and_tracking_range() {
        let mut cp = ConstantPool::new();
        let utf_clinit = push_utf8(&mut cp, "<clinit>");
        let utf_void = push_utf8(&mut cp, "()V");
        let utf_code = push_utf8(&mut cp, "Code");

        let cls_builder = push_class(&mut cp, "net/minecraft/world/entity/EntityType$Builder");
        let nat_sized = push_name_and_type(
            &mut cp,
            "sized",
            "(FF)Lnet/minecraft/world/entity/EntityType$Builder;",
        );
        let mref_sized = push_method_ref(&mut cp, cls_builder, nat_sized);

        let nat_range = push_name_and_type(
            &mut cp,
            "clientTrackingRange",
            "(I)Lnet/minecraft/world/entity/EntityType$Builder;",
        );
        let mref_range = push_method_ref(&mut cp, cls_builder, nat_range);

        let nat_build = push_name_and_type(
            &mut cp,
            "build",
            "(Ljava/lang/String;)Lnet/minecraft/world/entity/EntityType;",
        );
        let mref_build = push_method_ref(&mut cp, cls_builder, nat_build);

        let str_wizard = push_string(&mut cp, "wizard");

        // Floats for sized: width 0.75, height 1.95
        cp.push(ConstantPoolEntry::Float(0.75));
        let float_width = (cp.len() - 1) as u16;
        cp.push(ConstantPoolEntry::Float(1.95));
        let float_height = (cp.len() - 1) as u16;

        let cls_mod = push_class(&mut cp, "com/example/ModEntities");

        let mut bytecode = Vec::new();
        // new EntityType$Builder
        bytecode.extend_from_slice(&[0xBB, (cls_builder >> 8) as u8, (cls_builder & 0xFF) as u8]);
        // dup
        bytecode.push(0x59);
        // ldc float_width
        bytecode.extend_from_slice(&[0x12, float_width as u8]);
        // ldc float_height
        bytecode.extend_from_slice(&[0x12, float_height as u8]);
        // invokevirtual sized
        bytecode.extend_from_slice(&[0xB6, (mref_sized >> 8) as u8, (mref_sized & 0xFF) as u8]);
        // bipush 80 (tracking range)
        bytecode.extend_from_slice(&[0x10, 80]);
        // invokevirtual clientTrackingRange
        bytecode.extend_from_slice(&[0xB6, (mref_range >> 8) as u8, (mref_range & 0xFF) as u8]);
        // ldc str_wizard
        bytecode.extend_from_slice(&[0x12, str_wizard as u8]);
        // invokevirtual build
        bytecode.extend_from_slice(&[0xB6, (mref_build >> 8) as u8, (mref_build & 0xFF) as u8]);
        // pop
        bytecode.push(0x57);
        // return
        bytecode.push(0xB1);

        let method = MethodInfo {
            access_flags: 0x0008,
            name_index: utf_clinit,
            descriptor_index: utf_void,
            name: "<clinit>".to_string(),
            descriptor: "()V".to_string(),
            attributes: vec![AttributeInfo {
                name_index: utf_code,
                name: "Code".to_string(),
                data: Vec::new(),
            }],
            code: Some(CodeInfo {
                max_stack: 4,
                max_locals: 0,
                bytecode,
            }),
        };

        let class_file = ClassFile {
            minor_version: 0,
            major_version: 61,
            constant_pool: cp,
            access_flags: 0x0001,
            this_class: cls_mod,
            super_class: 0,
            interfaces: Vec::new(),
            fields: Vec::new(),
            methods: vec![method],
            attributes: Vec::new(),
        };

        let meta = mock_meta();
        let mut ir = ContentIR::new(meta.clone());
        scan_entity_classes(&[class_file], &meta, &mut ir);

        assert_eq!(ir.entity_types.len(), 1);
        let wizard = &ir.entity_types[0];
        assert_eq!(wizard.id, "ebwizardry:wizard");
        assert_eq!(wizard.template, "living");
        assert_eq!(wizard.width, 0.75);
        assert_eq!(wizard.height, 1.95);
        assert_eq!(wizard.tracking_range, 80);
    }

    #[test]
    fn test_scan_fabric_entity_registration() {
        let mut cp = ConstantPool::new();
        let utf_clinit = push_utf8(&mut cp, "<clinit>");
        let utf_void = push_utf8(&mut cp, "()V");
        let utf_code = push_utf8(&mut cp, "Code");

        let cls_registry = push_class(&mut cp, "net/minecraft/core/Registry");
        let nat_register = push_name_and_type(
            &mut cp,
            "register",
            "(Lnet/minecraft/core/Registry;Ljava/lang/String;Ljava/lang/Object;)Ljava/lang/Object;",
        );
        let mref_register = push_method_ref(&mut cp, cls_registry, nat_register);

        let cls_registries = push_class(&mut cp, "net/minecraft/core/registries/BuiltInRegistries");
        let nat_entity_type = push_name_and_type(
            &mut cp,
            "ENTITY_TYPE",
            "Lnet/minecraft/core/Registry;",
        );
        let fref_entity_type = push_field_ref(&mut cp, cls_registries, nat_entity_type);

        let str_bullet = push_string(&mut cp, "fire_bullet");
        let cls_mod = push_class(&mut cp, "com/example/FabricEntities");

        let mut bytecode = Vec::new();
        // getstatic BuiltInRegistries.ENTITY_TYPE
        bytecode.extend_from_slice(&[0xB2, (fref_entity_type >> 8) as u8, (fref_entity_type & 0xFF) as u8]);
        // ldc str_bullet
        bytecode.extend_from_slice(&[0x12, str_bullet as u8]);
        // aconst_null
        bytecode.push(0x01);
        // invokestatic Registry.register
        bytecode.extend_from_slice(&[0xB8, (mref_register >> 8) as u8, (mref_register & 0xFF) as u8]);
        // pop
        bytecode.push(0x57);
        // return
        bytecode.push(0xB1);

        let method = MethodInfo {
            access_flags: 0x0008,
            name_index: utf_clinit,
            descriptor_index: utf_void,
            name: "<clinit>".to_string(),
            descriptor: "()V".to_string(),
            attributes: vec![AttributeInfo {
                name_index: utf_code,
                name: "Code".to_string(),
                data: Vec::new(),
            }],
            code: Some(CodeInfo {
                max_stack: 4,
                max_locals: 0,
                bytecode,
            }),
        };

        let class_file = ClassFile {
            minor_version: 0,
            major_version: 61,
            constant_pool: cp,
            access_flags: 0x0001,
            this_class: cls_mod,
            super_class: 0,
            interfaces: Vec::new(),
            fields: Vec::new(),
            methods: vec![method],
            attributes: Vec::new(),
        };

        let meta = ModMetadata {
            id: "wizardry_fabric".to_string(),
            name: "Wizardry Fabric".to_string(),
            version: "1.0.0".to_string(),
            description: "Fabric spells".to_string(),
            authors: vec!["FabricDev".to_string()],
            loader: ModLoader::Fabric,
        };

        let mut ir = ContentIR::new(meta.clone());
        scan_entity_classes(&[class_file], &meta, &mut ir);

        assert_eq!(ir.entity_types.len(), 1);
        let bullet = &ir.entity_types[0];
        assert_eq!(bullet.id, "wizardry_fabric:fire_bullet");
        assert_eq!(bullet.template, "projectile");
        assert_eq!(bullet.width, 0.5);
        assert_eq!(bullet.height, 0.5);
        assert_eq!(bullet.tracking_range, 64);
    }
}

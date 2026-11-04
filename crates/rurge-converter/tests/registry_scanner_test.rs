use rurge_converter::manifest::{
    parse_fabric_manifest, parse_forge_manifest, parse_legacy_manifest, parse_manifest_from_zip,
    ModLoader, ModMetadata,
};
use std::io::{Cursor, Write};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

#[test]
fn test_parse_fabric_manifest_standard() {
    let json = r#"{
        "schemaVersion": 1,
        "id": "copper_plus",
        "version": "1.2.3",
        "name": "Copper Plus",
        "description": "Adds more copper utility items and blocks.",
        "authors": [
            "Alice",
            {
                "name": "Bob",
                "contact": { "sources": "https://github.com/example/copper_plus" }
            }
        ],
        "license": "MIT",
        "environment": "*"
    }"#;

    let meta = parse_fabric_manifest(json).expect("Fabric manifest should parse successfully");
    assert_eq!(meta.id, "copper_plus");
    assert_eq!(meta.name, "Copper Plus");
    assert_eq!(meta.version, "1.2.3");
    assert_eq!(meta.authors, vec!["Alice".to_string(), "Bob".to_string()]);
    assert_eq!(meta.description, "Adds more copper utility items and blocks.");
    assert_eq!(meta.loader, ModLoader::Fabric);
}

#[test]
fn test_parse_fabric_manifest_minimal() {
    let json = r#"{
        "id": "simple_mod"
    }"#;

    let meta = parse_fabric_manifest(json).expect("Minimal fabric manifest should parse");
    assert_eq!(meta.id, "simple_mod");
    assert_eq!(meta.name, "simple_mod");
    assert_eq!(meta.version, "1.0.0");
    assert!(meta.authors.is_empty());
    assert_eq!(meta.description, "");
    assert_eq!(meta.loader, ModLoader::Fabric);
}

#[test]
fn test_parse_forge_manifest_standard() {
    let toml = r#"
modLoader="javafml"
loaderVersion="[40,)"
license="MIT"

[[mods]]
modId="iron_furnaces"
version="2.1.0"
displayName="Iron Furnaces"
authors="AuthorOne, AuthorTwo"
description='''
Upgradable furnaces mod for Minecraft Forge.
'''
"#;

    let meta = parse_forge_manifest(toml).expect("Forge mods.toml should parse");
    assert_eq!(meta.id, "iron_furnaces");
    assert_eq!(meta.name, "Iron Furnaces");
    assert_eq!(meta.version, "2.1.0");
    assert_eq!(meta.authors, vec!["AuthorOne".to_string(), "AuthorTwo".to_string()]);
    assert!(meta.description.contains("Upgradable furnaces"));
    assert_eq!(meta.loader, ModLoader::Forge);
}

#[test]
fn test_parse_neoforge_manifest() {
    let toml = r#"
modLoader="neoforge"
loaderVersion="[20.4,)"

[[mods]]
modId="neo_copper"
version="3.0.0"
displayName="Neo Copper"
authors=["NeoDev"]
description="NeoForge edition of copper items."
"#;

    let meta = parse_forge_manifest(toml).expect("NeoForge mods.toml should parse");
    assert_eq!(meta.id, "neo_copper");
    assert_eq!(meta.name, "Neo Copper");
    assert_eq!(meta.version, "3.0.0");
    assert_eq!(meta.authors, vec!["NeoDev".to_string()]);
    assert_eq!(meta.loader, ModLoader::NeoForge);
}

#[test]
fn test_parse_legacy_manifest_array() {
    let json = r#"[
        {
            "modid": "classic_ore",
            "name": "Classic Ore",
            "version": "1.12.2-0.5",
            "description": "Old-school ores.",
            "authorList": ["VintageCrafter"]
        }
    ]"#;

    let meta = parse_legacy_manifest(json).expect("mcmod.info array should parse");
    assert_eq!(meta.id, "classic_ore");
    assert_eq!(meta.name, "Classic Ore");
    assert_eq!(meta.version, "1.12.2-0.5");
    assert_eq!(meta.authors, vec!["VintageCrafter".to_string()]);
    assert_eq!(meta.description, "Old-school ores.");
    assert_eq!(meta.loader, ModLoader::LegacyForge);
}

#[test]
fn test_parse_legacy_manifest_object() {
    let json = r#"{
        "modListVersion": 2,
        "modList": [
            {
                "modid": "classic_tools",
                "name": "Classic Tools",
                "version": "1.0",
                "description": "More tools.",
                "authorList": ["CrafterA", "CrafterB"]
            }
        ]
    }"#;

    let meta = parse_legacy_manifest(json).expect("mcmod.info object should parse");
    assert_eq!(meta.id, "classic_tools");
    assert_eq!(meta.name, "Classic Tools");
    assert_eq!(meta.version, "1.0");
    assert_eq!(meta.authors, vec!["CrafterA".to_string(), "CrafterB".to_string()]);
    assert_eq!(meta.loader, ModLoader::LegacyForge);
}

#[test]
fn test_parse_manifest_from_zip_fabric() {
    let mut buffer = Vec::new();
    {
        let mut zip = ZipWriter::new(Cursor::new(&mut buffer));
        zip.start_file("fabric.mod.json", SimpleFileOptions::default()).unwrap();
        zip.write_all(br#"{"id": "zip_fabric_mod", "name": "Zip Fabric", "version": "1.0.0"}"#).unwrap();
        zip.finish().unwrap();
    }

    let mut archive = zip::ZipArchive::new(Cursor::new(buffer)).unwrap();
    let meta = parse_manifest_from_zip(&mut archive).expect("Should detect fabric.mod.json");
    assert_eq!(meta.id, "zip_fabric_mod");
    assert_eq!(meta.loader, ModLoader::Fabric);
}

#[test]
fn test_parse_manifest_from_zip_forge() {
    let mut buffer = Vec::new();
    {
        let mut zip = ZipWriter::new(Cursor::new(&mut buffer));
        zip.start_file("META-INF/mods.toml", SimpleFileOptions::default()).unwrap();
        zip.write_all(br#"
[[mods]]
modId = "zip_forge_mod"
version = "2.0.0"
displayName = "Zip Forge"
"#).unwrap();
        zip.finish().unwrap();
    }

    let mut archive = zip::ZipArchive::new(Cursor::new(buffer)).unwrap();
    let meta = parse_manifest_from_zip(&mut archive).expect("Should detect META-INF/mods.toml");
    assert_eq!(meta.id, "zip_forge_mod");
    assert_eq!(meta.loader, ModLoader::Forge);
}

#[test]
fn test_parse_manifest_from_zip_unrecognized() {
    let mut buffer = Vec::new();
    {
        let mut zip = ZipWriter::new(Cursor::new(&mut buffer));
        zip.start_file("dummy.txt", SimpleFileOptions::default()).unwrap();
        zip.write_all(b"not a mod manifest").unwrap();
        zip.finish().unwrap();
    }

    let mut archive = zip::ZipArchive::new(Cursor::new(buffer)).unwrap();
    let result = parse_manifest_from_zip(&mut archive);
    assert!(result.is_err());
}

use rurge_converter::bytecode::ClassFile;
use rurge_converter::patterns::fabric::scan_fabric_classes;
use rurge_converter::patterns::forge::scan_forge_classes;
use rurge_converter::patterns::scan_classes_for_content;
use rurge_converter::ContentIR;
use rurge_converter::ToolType;

struct SyntheticClassBuilder {
    cp_entries: Vec<Vec<u8>>,
    this_class: u16,
    super_class: u16,
    methods: Vec<Vec<u8>>,
}

impl SyntheticClassBuilder {
    fn new() -> Self {
        Self {
            cp_entries: Vec::new(),
            this_class: 0,
            super_class: 0,
            methods: Vec::new(),
        }
    }

    fn add_utf8(&mut self, s: &str) -> u16 {
        let mut entry = Vec::new();
        entry.push(1);
        let bytes = s.as_bytes();
        entry.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
        entry.extend_from_slice(bytes);
        self.cp_entries.push(entry);
        self.cp_entries.len() as u16
    }

    fn add_float(&mut self, val: f32) -> u16 {
        let mut entry = Vec::new();
        entry.push(4);
        entry.extend_from_slice(&val.to_be_bytes());
        self.cp_entries.push(entry);
        self.cp_entries.len() as u16
    }

    fn add_class(&mut self, name: &str) -> u16 {
        let name_idx = self.add_utf8(name);
        let mut entry = Vec::new();
        entry.push(7);
        entry.extend_from_slice(&name_idx.to_be_bytes());
        self.cp_entries.push(entry);
        self.cp_entries.len() as u16
    }

    fn add_string(&mut self, s: &str) -> u16 {
        let utf8_idx = self.add_utf8(s);
        let mut entry = Vec::new();
        entry.push(8);
        entry.extend_from_slice(&utf8_idx.to_be_bytes());
        self.cp_entries.push(entry);
        self.cp_entries.len() as u16
    }

    fn add_name_and_type(&mut self, name: &str, descriptor: &str) -> u16 {
        let name_idx = self.add_utf8(name);
        let desc_idx = self.add_utf8(descriptor);
        let mut entry = Vec::new();
        entry.push(12);
        entry.extend_from_slice(&name_idx.to_be_bytes());
        entry.extend_from_slice(&desc_idx.to_be_bytes());
        self.cp_entries.push(entry);
        self.cp_entries.len() as u16
    }

    fn add_method_ref(&mut self, class_name: &str, method_name: &str, descriptor: &str) -> u16 {
        let class_idx = self.add_class(class_name);
        let nt_idx = self.add_name_and_type(method_name, descriptor);
        let mut entry = Vec::new();
        entry.push(10);
        entry.extend_from_slice(&class_idx.to_be_bytes());
        entry.extend_from_slice(&nt_idx.to_be_bytes());
        self.cp_entries.push(entry);
        self.cp_entries.len() as u16
    }

    fn add_field_ref(&mut self, class_name: &str, field_name: &str, descriptor: &str) -> u16 {
        let class_idx = self.add_class(class_name);
        let nt_idx = self.add_name_and_type(field_name, descriptor);
        let mut entry = Vec::new();
        entry.push(9);
        entry.extend_from_slice(&class_idx.to_be_bytes());
        entry.extend_from_slice(&nt_idx.to_be_bytes());
        self.cp_entries.push(entry);
        self.cp_entries.len() as u16
    }

    fn set_classes(&mut self, this_class_name: &str, super_class_name: &str) {
        self.this_class = self.add_class(this_class_name);
        self.super_class = self.add_class(super_class_name);
    }

    fn add_method(&mut self, name: &str, descriptor: &str, bytecode: &[u8]) {
        let name_idx = self.add_utf8(name);
        let desc_idx = self.add_utf8(descriptor);
        let code_utf8_idx = self.add_utf8("Code");

        let mut code_attr = Vec::new();
        code_attr.extend_from_slice(&code_utf8_idx.to_be_bytes());
        let attr_length = (2 + 2 + 4 + bytecode.len() + 2 + 2) as u32;
        code_attr.extend_from_slice(&attr_length.to_be_bytes());
        code_attr.extend_from_slice(&10u16.to_be_bytes()); // max_stack
        code_attr.extend_from_slice(&4u16.to_be_bytes());  // max_locals
        code_attr.extend_from_slice(&(bytecode.len() as u32).to_be_bytes());
        code_attr.extend_from_slice(bytecode);
        code_attr.extend_from_slice(&0u16.to_be_bytes()); // exception_table_length
        code_attr.extend_from_slice(&0u16.to_be_bytes()); // attributes_count

        let mut method = Vec::new();
        method.extend_from_slice(&0x0009u16.to_be_bytes()); // public static
        method.extend_from_slice(&name_idx.to_be_bytes());
        method.extend_from_slice(&desc_idx.to_be_bytes());
        method.extend_from_slice(&1u16.to_be_bytes()); // 1 attribute: Code
        method.extend_from_slice(&code_attr);

        self.methods.push(method);
    }

    fn build(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&0xCAFEBABEu32.to_be_bytes());
        buf.extend_from_slice(&0u16.to_be_bytes());  // minor
        buf.extend_from_slice(&65u16.to_be_bytes()); // major (Java 21)

        let cp_count = (self.cp_entries.len() + 1) as u16;
        buf.extend_from_slice(&cp_count.to_be_bytes());
        for entry in &self.cp_entries {
            buf.extend_from_slice(entry);
        }

        buf.extend_from_slice(&0x0021u16.to_be_bytes()); // public super
        buf.extend_from_slice(&self.this_class.to_be_bytes());
        buf.extend_from_slice(&self.super_class.to_be_bytes());
        buf.extend_from_slice(&0u16.to_be_bytes()); // interfaces_count
        buf.extend_from_slice(&0u16.to_be_bytes()); // fields_count

        buf.extend_from_slice(&(self.methods.len() as u16).to_be_bytes());
        for m in &self.methods {
            buf.extend_from_slice(m);
        }

        buf.extend_from_slice(&0u16.to_be_bytes()); // attributes_count
        buf
    }
}

#[test]
fn test_fabric_registry_scanner_block_and_item() {
    let mut builder = SyntheticClassBuilder::new();
    builder.set_classes("com/example/copperplus/ModInit", "java/lang/Object");

    // Constant pool entries
    let block_field = builder.add_field_ref(
        "net/minecraft/util/registry/Registry",
        "BLOCK",
        "Lnet/minecraft/util/registry/Registry;",
    );
    let item_field = builder.add_field_ref(
        "net/minecraft/util/registry/Registry",
        "ITEM",
        "Lnet/minecraft/util/registry/Registry;",
    );

    let id_class = builder.add_class("net/minecraft/util/Identifier");
    let id_init = builder.add_method_ref(
        "net/minecraft/util/Identifier",
        "<init>",
        "(Ljava/lang/String;Ljava/lang/String;)V",
    );

    let block_class = builder.add_class("net/minecraft/block/Block");
    let block_init = builder.add_method_ref(
        "net/minecraft/block/Block",
        "<init>",
        "(Lnet/minecraft/block/AbstractBlock$Settings;)V",
    );

    let settings_of = builder.add_method_ref(
        "net/fabricmc/fabric/api/object/builder/v1/block/FabricBlockSettings",
        "of",
        "()Lnet/fabricmc/fabric/api/object/builder/v1/block/FabricBlockSettings;",
    );
    let strength_mth = builder.add_method_ref(
        "net/fabricmc/fabric/api/object/builder/v1/block/FabricBlockSettings",
        "strength",
        "(FF)Lnet/fabricmc/fabric/api/object/builder/v1/block/FabricBlockSettings;",
    );
    let luminance_mth = builder.add_method_ref(
        "net/fabricmc/fabric/api/object/builder/v1/block/FabricBlockSettings",
        "luminance",
        "(I)Lnet/fabricmc/fabric/api/object/builder/v1/block/FabricBlockSettings;",
    );
    let non_opaque_mth = builder.add_method_ref(
        "net/fabricmc/fabric/api/object/builder/v1/block/FabricBlockSettings",
        "nonOpaque",
        "()Lnet/fabricmc/fabric/api/object/builder/v1/block/FabricBlockSettings;",
    );

    let reg_register = builder.add_method_ref(
        "net/minecraft/util/registry/Registry",
        "register",
        "(Lnet/minecraft/util/registry/Registry;Lnet/minecraft/util/Identifier;Ljava/lang/Object;)Ljava/lang/Object;",
    );

    let modid_str = builder.add_string("copper_plus");
    let block_name_str = builder.add_string("copper_block_fancy");
    let item_name_str = builder.add_string("copper_wrench");

    let f_hardness = builder.add_float(3.0);
    let f_resistance = builder.add_float(6.0);

    let item_class = builder.add_class("net/minecraft/item/Item");
    let item_init = builder.add_method_ref(
        "net/minecraft/item/Item",
        "<init>",
        "(Lnet/minecraft/item/Item$Settings;)V",
    );
    let item_settings_class = builder.add_class("net/fabricmc/fabric/api/item/v1/FabricItemSettings");
    let item_settings_init = builder.add_method_ref(
        "net/fabricmc/fabric/api/item/v1/FabricItemSettings",
        "<init>",
        "()V",
    );
    let max_count_mth = builder.add_method_ref(
        "net/fabricmc/fabric/api/item/v1/FabricItemSettings",
        "maxCount",
        "(I)Lnet/fabricmc/fabric/api/item/v1/FabricItemSettings;",
    );
    let max_damage_mth = builder.add_method_ref(
        "net/fabricmc/fabric/api/item/v1/FabricItemSettings",
        "maxDamage",
        "(I)Lnet/fabricmc/fabric/api/item/v1/FabricItemSettings;",
    );

    let mut code = Vec::new();
    // 1. Register Block
    code.push(0xB2); // getstatic Registry.BLOCK
    code.extend_from_slice(&block_field.to_be_bytes());

    code.push(0xBB); // new Identifier
    code.extend_from_slice(&id_class.to_be_bytes());
    code.push(0x59); // dup
    code.push(0x12); // ldc "copper_plus"
    code.push(modid_str as u8);
    code.push(0x12); // ldc "copper_block_fancy"
    code.push(block_name_str as u8);
    code.push(0xB7); // invokespecial Identifier.<init>
    code.extend_from_slice(&id_init.to_be_bytes());

    code.push(0xBB); // new Block
    code.extend_from_slice(&block_class.to_be_bytes());
    code.push(0x59); // dup
    code.push(0xB8); // invokestatic FabricBlockSettings.of
    code.extend_from_slice(&settings_of.to_be_bytes());
    code.push(0x12); // ldc 3.0f
    code.push(f_hardness as u8);
    code.push(0x12); // ldc 6.0f
    code.push(f_resistance as u8);
    code.push(0xB6); // invokevirtual strength(FF)
    code.extend_from_slice(&strength_mth.to_be_bytes());
    code.push(0x10); // bipush 15
    code.push(15);
    code.push(0xB6); // invokevirtual luminance(I)
    code.extend_from_slice(&luminance_mth.to_be_bytes());
    code.push(0xB6); // invokevirtual nonOpaque()
    code.extend_from_slice(&non_opaque_mth.to_be_bytes());
    code.push(0xB7); // invokespecial Block.<init>
    code.extend_from_slice(&block_init.to_be_bytes());

    code.push(0xB8); // invokestatic Registry.register
    code.extend_from_slice(&reg_register.to_be_bytes());
    code.push(0x57); // pop

    // 2. Register Item
    code.push(0xB2); // getstatic Registry.ITEM
    code.extend_from_slice(&item_field.to_be_bytes());

    code.push(0xBB); // new Identifier
    code.extend_from_slice(&id_class.to_be_bytes());
    code.push(0x59); // dup
    code.push(0x12); // ldc "copper_plus"
    code.push(modid_str as u8);
    code.push(0x12); // ldc "copper_wrench"
    code.push(item_name_str as u8);
    code.push(0xB7); // invokespecial Identifier.<init>
    code.extend_from_slice(&id_init.to_be_bytes());

    code.push(0xBB); // new Item
    code.extend_from_slice(&item_class.to_be_bytes());
    code.push(0x59); // dup
    code.push(0xBB); // new FabricItemSettings
    code.extend_from_slice(&item_settings_class.to_be_bytes());
    code.push(0x59); // dup
    code.push(0xB7); // invokespecial FabricItemSettings.<init>
    code.extend_from_slice(&item_settings_init.to_be_bytes());
    code.push(0x10); // bipush 16
    code.push(16);
    code.push(0xB6); // invokevirtual maxCount(16)
    code.extend_from_slice(&max_count_mth.to_be_bytes());
    code.push(0x11); // sipush 250
    code.extend_from_slice(&250u16.to_be_bytes());
    code.push(0xB6); // invokevirtual maxDamage(250)
    code.extend_from_slice(&max_damage_mth.to_be_bytes());
    code.push(0xB7); // invokespecial Item.<init>
    code.extend_from_slice(&item_init.to_be_bytes());

    code.push(0xB8); // invokestatic Registry.register
    code.extend_from_slice(&reg_register.to_be_bytes());
    code.push(0x57); // pop

    code.push(0xB1); // return

    builder.add_method("onInitialize", "()V", &code);
    let class_bytes = builder.build();
    let class = ClassFile::parse(&class_bytes).expect("Synthetic class should parse");

    let meta = ModMetadata {
        id: "copper_plus".to_string(),
        name: "Copper Plus".to_string(),
        version: "1.0.0".to_string(),
        authors: vec![],
        description: "".to_string(),
        loader: ModLoader::Fabric,
    };

    let mut ir = ContentIR::new(meta.clone());
    scan_fabric_classes(&[class], &meta, &mut ir);

    assert_eq!(ir.blocks.len(), 1, "Should detect 1 block");
    let block = ir.get_block("copper_plus:copper_block_fancy").expect("Block must exist");
    assert_eq!(block.hardness, 3.0);
    assert_eq!(block.resistance, 6.0);
    assert_eq!(block.light_emission, 15);
    assert!(!block.opaque);

    assert_eq!(ir.items.len(), 1, "Should detect 1 item");
    let item = ir.get_item("copper_plus:copper_wrench").expect("Item must exist");
    assert_eq!(item.max_stack_size, 16);
    assert_eq!(item.durability, 250);
}

#[test]
fn test_forge_registry_scanner_deferred_register() {
    let mut builder = SyntheticClassBuilder::new();
    builder.set_classes("com/example/furnaces/ModBlocks", "java/lang/Object");

    let forge_blocks_field = builder.add_field_ref(
        "net/minecraftforge/registrations/ForgeRegistries",
        "BLOCKS",
        "Lnet/minecraftforge/registrations/IForgeRegistry;",
    );

    let def_create = builder.add_method_ref(
        "net/minecraftforge/registrations/DeferredRegister",
        "create",
        "(Lnet/minecraftforge/registrations/IForgeRegistry;Ljava/lang/String;)Lnet/minecraftforge/registrations/DeferredRegister;",
    );

    let my_blocks_field = builder.add_field_ref(
        "com/example/furnaces/ModBlocks",
        "BLOCKS",
        "Lnet/minecraftforge/registrations/DeferredRegister;",
    );

    let def_register = builder.add_method_ref(
        "net/minecraftforge/registrations/DeferredRegister",
        "register",
        "(Ljava/lang/String;Ljava/util/function/Supplier;)Lnet/minecraftforge/registrations/RegistryObject;",
    );

    let modid_str = builder.add_string("iron_furnaces");
    let furnace_str = builder.add_string("copper_furnace");
    let f_hardness = builder.add_float(4.0);
    let f_resistance = builder.add_float(10.0);

    let block_class = builder.add_class("net/minecraft/world/level/block/Block");
    let block_init = builder.add_method_ref(
        "net/minecraft/world/level/block/Block",
        "<init>",
        "(Lnet/minecraft/world/level/block/state/BlockBehaviour$Properties;)V",
    );
    let prop_of = builder.add_method_ref(
        "net/minecraft/world/level/block/state/BlockBehaviour$Properties",
        "of",
        "()Lnet/minecraft/world/level/block/state/BlockBehaviour$Properties;",
    );
    let strength_mth = builder.add_method_ref(
        "net/minecraft/world/level/block/state/BlockBehaviour$Properties",
        "strength",
        "(FF)Lnet/minecraft/world/level/block/state/BlockBehaviour$Properties;",
    );

    // <clinit>:
    // 1. BLOCKS = DeferredRegister.create(ForgeRegistries.BLOCKS, "iron_furnaces");
    // 2. BLOCKS.register("copper_furnace", ...);
    let mut clinit_code = Vec::new();
    clinit_code.push(0xB2); // getstatic ForgeRegistries.BLOCKS
    clinit_code.extend_from_slice(&forge_blocks_field.to_be_bytes());
    clinit_code.push(0x12); // ldc "iron_furnaces"
    clinit_code.push(modid_str as u8);
    clinit_code.push(0xB8); // invokestatic DeferredRegister.create
    clinit_code.extend_from_slice(&def_create.to_be_bytes());
    clinit_code.push(0xB3); // putstatic ModBlocks.BLOCKS
    clinit_code.extend_from_slice(&my_blocks_field.to_be_bytes());

    clinit_code.push(0xB2); // getstatic ModBlocks.BLOCKS
    clinit_code.extend_from_slice(&my_blocks_field.to_be_bytes());
    clinit_code.push(0x12); // ldc "copper_furnace"
    clinit_code.push(furnace_str as u8);
    clinit_code.push(0x01); // aconst_null (dummy supplier)
    clinit_code.push(0xB6); // invokevirtual DeferredRegister.register
    clinit_code.extend_from_slice(&def_register.to_be_bytes());
    clinit_code.push(0x57); // pop
    clinit_code.push(0xB1); // return

    builder.add_method("<clinit>", "()V", &clinit_code);

    // Supplier method lambda$static$0(): Block
    let mut lambda_code = Vec::new();
    lambda_code.push(0xBB); // new Block
    lambda_code.extend_from_slice(&block_class.to_be_bytes());
    lambda_code.push(0x59); // dup
    lambda_code.push(0xB8); // invokestatic Properties.of
    lambda_code.extend_from_slice(&prop_of.to_be_bytes());
    lambda_code.push(0x12); // ldc 4.0f
    lambda_code.push(f_hardness as u8);
    lambda_code.push(0x12); // ldc 10.0f
    lambda_code.push(f_resistance as u8);
    lambda_code.push(0xB6); // invokevirtual strength(4.0f, 10.0f)
    lambda_code.extend_from_slice(&strength_mth.to_be_bytes());
    lambda_code.push(0xB7); // invokespecial Block.<init>
    lambda_code.extend_from_slice(&block_init.to_be_bytes());
    lambda_code.push(0xB0); // areturn

    builder.add_method("lambda$static$0", "()Lnet/minecraft/world/level/block/Block;", &lambda_code);

    let class_bytes = builder.build();
    let class = ClassFile::parse(&class_bytes).expect("Synthetic class should parse");

    let meta = ModMetadata {
        id: "iron_furnaces".to_string(),
        name: "Iron Furnaces".to_string(),
        version: "1.0.0".to_string(),
        authors: vec![],
        description: "".to_string(),
        loader: ModLoader::Forge,
    };

    let mut ir = ContentIR::new(meta.clone());
    scan_forge_classes(&[class], &meta, &mut ir);

    assert_eq!(ir.blocks.len(), 1, "Should detect 1 Forge block");
    let block = ir.get_block("iron_furnaces:copper_furnace").expect("Block must exist");
    assert_eq!(block.hardness, 4.0);
    assert_eq!(block.resistance, 10.0);
}

#[test]
fn test_scan_classes_for_content_routes_correctly() {
    // 1. Fabric class
    let mut f_builder = SyntheticClassBuilder::new();
    f_builder.set_classes("com/example/fab/Mod", "java/lang/Object");
    let block_field = f_builder.add_field_ref("net/minecraft/util/registry/Registry", "BLOCK", "Lnet/minecraft/util/registry/Registry;");
    let reg_register = f_builder.add_method_ref("net/minecraft/util/registry/Registry", "register", "(Lnet/minecraft/util/registry/Registry;Lnet/minecraft/util/Identifier;Ljava/lang/Object;)Ljava/lang/Object;");
    let id_class = f_builder.add_class("net/minecraft/util/Identifier");
    let id_init = f_builder.add_method_ref("net/minecraft/util/Identifier", "<init>", "(Ljava/lang/String;Ljava/lang/String;)V");
    let block_class = f_builder.add_class("net/minecraft/block/Block");
    let block_init = f_builder.add_method_ref("net/minecraft/block/Block", "<init>", "(Lnet/minecraft/block/AbstractBlock$Settings;)V");
    let modid_str = f_builder.add_string("fab_mod");
    let block_str = f_builder.add_string("fab_block");

    let mut code = Vec::new();
    code.push(0xB2); code.extend_from_slice(&block_field.to_be_bytes());
    code.push(0xBB); code.extend_from_slice(&id_class.to_be_bytes());
    code.push(0x59);
    code.push(0x12); code.push(modid_str as u8);
    code.push(0x12); code.push(block_str as u8);
    code.push(0xB7); code.extend_from_slice(&id_init.to_be_bytes());
    code.push(0xBB); code.extend_from_slice(&block_class.to_be_bytes());
    code.push(0x59);
    code.push(0x01); // aconst_null
    code.push(0xB7); code.extend_from_slice(&block_init.to_be_bytes());
    code.push(0xB8); code.extend_from_slice(&reg_register.to_be_bytes());
    code.push(0x57);
    code.push(0xB1);
    f_builder.add_method("onInitialize", "()V", &code);

    let fab_class = ClassFile::parse(&f_builder.build()).unwrap();

    let meta = ModMetadata {
        id: "fab_mod".to_string(),
        name: "Fabric Mod".to_string(),
        version: "1.0.0".to_string(),
        authors: vec!["Alice".to_string()],
        description: "A fabric mod".to_string(),
        loader: ModLoader::Fabric,
    };

    let ir = scan_classes_for_content(&[fab_class], &meta);
    assert_eq!(ir.blocks.len(), 1);
    assert_eq!(ir.blocks[0].id, "fab_mod:fab_block");
}

#[test]
fn test_fabric_tool_and_food_item_scanning() {
    let mut builder = SyntheticClassBuilder::new();
    builder.set_classes("com/example/tools/Mod", "java/lang/Object");

    let item_field = builder.add_field_ref("net/minecraft/util/registry/Registry", "ITEM", "Lnet/minecraft/util/registry/Registry;");
    let reg_register = builder.add_method_ref("net/minecraft/util/registry/Registry", "register", "(Lnet/minecraft/util/registry/Registry;Lnet/minecraft/util/Identifier;Ljava/lang/Object;)Ljava/lang/Object;");
    let id_class = builder.add_class("net/minecraft/util/Identifier");
    let id_init = builder.add_method_ref("net/minecraft/util/Identifier", "<init>", "(Ljava/lang/String;Ljava/lang/String;)V");

    let pickaxe_class = builder.add_class("net/minecraft/item/PickaxeItem");
    let pickaxe_init = builder.add_method_ref("net/minecraft/item/PickaxeItem", "<init>", "()V");

    let food_class = builder.add_class("net/minecraft/item/Item");
    let food_init = builder.add_method_ref("net/minecraft/item/Item", "<init>", "()V");
    let food_method = builder.add_method_ref("net/fabricmc/fabric/api/item/v1/FabricItemSettings", "food", "()Lnet/fabricmc/fabric/api/item/v1/FabricItemSettings;");

    let modid = builder.add_string("tool_mod");
    let pick_name = builder.add_string("copper_pickaxe");
    let apple_name = builder.add_string("copper_apple");

    let mut code = Vec::new();
    // 1. Pickaxe
    code.push(0xB2); code.extend_from_slice(&item_field.to_be_bytes());
    code.push(0xBB); code.extend_from_slice(&id_class.to_be_bytes());
    code.push(0x59);
    code.push(0x12); code.push(modid as u8);
    code.push(0x12); code.push(pick_name as u8);
    code.push(0xB7); code.extend_from_slice(&id_init.to_be_bytes());
    code.push(0xBB); code.extend_from_slice(&pickaxe_class.to_be_bytes());
    code.push(0x59);
    code.push(0xB7); code.extend_from_slice(&pickaxe_init.to_be_bytes());
    code.push(0xB8); code.extend_from_slice(&reg_register.to_be_bytes());
    code.push(0x57);

    // 2. Food Item
    code.push(0xB2); code.extend_from_slice(&item_field.to_be_bytes());
    code.push(0xBB); code.extend_from_slice(&id_class.to_be_bytes());
    code.push(0x59);
    code.push(0x12); code.push(modid as u8);
    code.push(0x12); code.push(apple_name as u8);
    code.push(0xB7); code.extend_from_slice(&id_init.to_be_bytes());
    code.push(0xBB); code.extend_from_slice(&food_class.to_be_bytes());
    code.push(0x59);
    code.push(0xB6); code.extend_from_slice(&food_method.to_be_bytes());
    code.push(0xB7); code.extend_from_slice(&food_init.to_be_bytes());
    code.push(0xB8); code.extend_from_slice(&reg_register.to_be_bytes());
    code.push(0x57);

    code.push(0xB1);
    builder.add_method("onInitialize", "()V", &code);

    let class = ClassFile::parse(&builder.build()).unwrap();
    let meta = ModMetadata {
        id: "tool_mod".to_string(),
        name: "Tool Mod".to_string(),
        version: "1.0.0".to_string(),
        authors: vec![],
        description: "".to_string(),
        loader: ModLoader::Fabric,
    };

    let ir = scan_classes_for_content(&[class], &meta);
    assert_eq!(ir.items.len(), 2);

    let pick = ir.get_item("tool_mod:copper_pickaxe").expect("Pickaxe item should exist");
    assert!(pick.tool.is_some());
    let tool_stats = pick.tool.as_ref().unwrap();
    assert_eq!(tool_stats.tool_type, ToolType::Pickaxe);

    let apple = ir.get_item("tool_mod:copper_apple").expect("Food item should exist");
    assert!(apple.food.is_some());
    let food_stats = apple.food.as_ref().unwrap();
    assert_eq!(food_stats.nutrition, 4);
}


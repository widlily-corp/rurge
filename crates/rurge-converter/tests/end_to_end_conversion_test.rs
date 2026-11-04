use rurge_converter::{
    convert_jar_to_rmod, inspect_jar, CompatibilityStatus, ContentSchema, ModLoader, RmodManifest,
};
use rurge_core::ModManifest;
use std::fs::File;
use std::io::{Read, Write};
use tempfile::tempdir;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

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

/// Helper creating a synthetic test JAR archive with Fabric manifest, classes, models, textures, recipes, and lang.
fn create_test_fabric_jar(path: &std::path::Path) {
    let file = File::create(path).expect("failed to create test jar file");
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default();

    // 1. fabric.mod.json
    let fabric_json = r#"{
        "schemaVersion": 1,
        "id": "emerald_craft",
        "name": "Emerald Craft",
        "version": "1.2.0",
        "description": "Emerald tools and blocks for native Minecraft.",
        "authors": ["Steve", "Alex"],
        "license": "MIT",
        "environment": "*"
    }"#;
    zip.start_file("fabric.mod.json", options).unwrap();
    zip.write_all(fabric_json.as_bytes()).unwrap();

    // 2. Classfile registering emerald_lantern (block) and emerald_chisel (item)
    let mut builder = SyntheticClassBuilder::new();
    builder.set_classes("com/example/emerald/EmeraldMod", "java/lang/Object");

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
    let reg_register = builder.add_method_ref(
        "net/minecraft/util/registry/Registry",
        "register",
        "(Lnet/minecraft/util/registry/Registry;Lnet/minecraft/util/Identifier;Ljava/lang/Object;)Ljava/lang/Object;",
    );
    let block_class = builder.add_class("net/minecraft/block/Block");
    let block_init = builder.add_method_ref("net/minecraft/block/Block", "<init>", "()V");
    let item_class = builder.add_class("net/minecraft/item/Item");
    let item_init = builder.add_method_ref("net/minecraft/item/Item", "<init>", "()V");

    let modid_str = builder.add_string("emerald_craft");
    let lantern_str = builder.add_string("emerald_lantern");
    let chisel_str = builder.add_string("emerald_chisel");

    let mut code = Vec::new();
    // Register Block: Registry.register(Registry.BLOCK, new Identifier("emerald_craft", "emerald_lantern"), new Block())
    code.push(0xB2); code.extend_from_slice(&block_field.to_be_bytes()); // getstatic BLOCK
    code.push(0xBB); code.extend_from_slice(&id_class.to_be_bytes());    // new Identifier
    code.push(0x59);                                                     // dup
    code.push(0x13); code.extend_from_slice(&modid_str.to_be_bytes());   // ldc_w modid
    code.push(0x13); code.extend_from_slice(&lantern_str.to_be_bytes()); // ldc_w lantern
    code.push(0xB7); code.extend_from_slice(&id_init.to_be_bytes());     // invokespecial <init>
    code.push(0xBB); code.extend_from_slice(&block_class.to_be_bytes()); // new Block
    code.push(0x59);                                                     // dup
    code.push(0xB7); code.extend_from_slice(&block_init.to_be_bytes());  // invokespecial <init>
    code.push(0xB8); code.extend_from_slice(&reg_register.to_be_bytes());// invokestatic register
    code.push(0x57);                                                     // pop

    // Register Item: Registry.register(Registry.ITEM, new Identifier("emerald_craft", "emerald_chisel"), new Item())
    code.push(0xB2); code.extend_from_slice(&item_field.to_be_bytes());  // getstatic ITEM
    code.push(0xBB); code.extend_from_slice(&id_class.to_be_bytes());    // new Identifier
    code.push(0x59);                                                     // dup
    code.push(0x13); code.extend_from_slice(&modid_str.to_be_bytes());   // ldc_w modid
    code.push(0x13); code.extend_from_slice(&chisel_str.to_be_bytes());  // ldc_w chisel
    code.push(0xB7); code.extend_from_slice(&id_init.to_be_bytes());     // invokespecial <init>
    code.push(0xBB); code.extend_from_slice(&item_class.to_be_bytes());  // new Item
    code.push(0x59);                                                     // dup
    code.push(0xB7); code.extend_from_slice(&item_init.to_be_bytes());   // invokespecial <init>
    code.push(0xB8); code.extend_from_slice(&reg_register.to_be_bytes());// invokestatic register
    code.push(0x57);                                                     // pop
    code.push(0xB1);                                                     // return

    builder.add_method("onInitialize", "()V", &code);
    let class_bytes = builder.build();

    zip.start_file("com/example/emerald/EmeraldMod.class", options).unwrap();
    zip.write_all(&class_bytes).unwrap();

    // 3. Blockstate JSON
    let blockstate_json = r#"{"variants": {"": {"model": "emerald_craft:block/emerald_lantern"}}}"#;
    zip.start_file("assets/emerald_craft/blockstates/emerald_lantern.json", options).unwrap();
    zip.write_all(blockstate_json.as_bytes()).unwrap();

    // 4. Block Model JSON
    let block_model_json = r#"{"parent": "minecraft:block/cube_all", "textures": {"all": "emerald_craft:block/emerald_lantern"}}"#;
    zip.start_file("assets/emerald_craft/models/block/emerald_lantern.json", options).unwrap();
    zip.write_all(block_model_json.as_bytes()).unwrap();

    // 5. Item Model JSON
    let item_model_json = r#"{"parent": "minecraft:item/generated", "textures": {"layer0": "emerald_craft:item/emerald_chisel"}}"#;
    zip.start_file("assets/emerald_craft/models/item/emerald_chisel.json", options).unwrap();
    zip.write_all(item_model_json.as_bytes()).unwrap();

    // 6. Textures
    let fake_png = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]; // PNG magic
    zip.start_file("assets/emerald_craft/textures/block/emerald_lantern.png", options).unwrap();
    zip.write_all(&fake_png).unwrap();
    zip.start_file("assets/emerald_craft/textures/item/emerald_chisel.png", options).unwrap();
    zip.write_all(&fake_png).unwrap();

    // 7. Lang JSON
    let lang_json = r#"{"block.emerald_craft.emerald_lantern": "Emerald Lantern", "item.emerald_craft.emerald_chisel": "Emerald Chisel"}"#;
    zip.start_file("assets/emerald_craft/lang/en_us.json", options).unwrap();
    zip.write_all(lang_json.as_bytes()).unwrap();

    // 8. Recipe JSON
    let recipe_json = r#"{
        "type": "minecraft:crafting_shaped",
        "pattern": [" E ", " S ", " S "],
        "key": {
            "E": { "item": "minecraft:emerald" },
            "S": { "item": "minecraft:stick" }
        },
        "result": {
            "item": "emerald_craft:emerald_chisel",
            "count": 1
        }
    }"#;
    zip.start_file("data/emerald_craft/recipes/emerald_chisel.json", options).unwrap();
    zip.write_all(recipe_json.as_bytes()).unwrap();

    zip.finish().unwrap();
}

#[test]
fn test_end_to_end_fabric_jar_conversion() {
    let temp = tempdir().expect("failed to create temp dir");
    let jar_path = temp.path().join("emerald-mod.jar");
    let rmod_path = temp.path().join("emerald-mod.rmod");

    create_test_fabric_jar(&jar_path);

    let summary = convert_jar_to_rmod(&jar_path, Some(&rmod_path))
        .expect("conversion should succeed");

    assert_eq!(summary.metadata.id, "emerald_craft");
    assert_eq!(summary.metadata.name, "Emerald Craft");
    assert_eq!(summary.metadata.version, "1.2.0");
    assert_eq!(summary.metadata.loader, ModLoader::Fabric);
    assert_eq!(summary.blocks_count, 1);
    assert_eq!(summary.items_count, 1);
    assert_eq!(summary.recipes_count, 1);
    assert_eq!(summary.textures_count, 2);
    assert_eq!(summary.models_count, 2);
    assert!(summary.lang_keys_count >= 2);
    assert_eq!(summary.compatibility.status, CompatibilityStatus::Full);

    // Verify .rmod file structure
    assert!(rmod_path.exists());
    let rmod_file = File::open(&rmod_path).expect("open rmod file");
    let mut archive = zip::ZipArchive::new(rmod_file).expect("open rmod zip");

    // Check manifest.toml
    let manifest_str = {
        let mut manifest_entry = archive.by_name("manifest.toml").expect("manifest.toml exists");
        let mut s = String::new();
        manifest_entry.read_to_string(&mut s).unwrap();
        s
    };
    assert!(manifest_str.contains("[mod]"));
    assert!(manifest_str.contains("id = \"emerald_craft\""));
    assert!(manifest_str.contains("[content]"));
    assert!(manifest_str.contains("schema = \"content.json\""));
    assert!(manifest_str.contains("data_driven = true"));

    let rmod_manifest: RmodManifest = toml::from_str(&manifest_str).expect("parse manifest.toml");
    assert_eq!(rmod_manifest.mod_section.id, "emerald_craft");
    assert_eq!(rmod_manifest.content.schema, "content.json");

    // Check rurge.toml compatibility
    let rurge_str = {
        let mut rurge_entry = archive.by_name("rurge.toml").expect("rurge.toml exists");
        let mut s = String::new();
        rurge_entry.read_to_string(&mut s).unwrap();
        s
    };
    let mod_manifest = ModManifest::from_toml_str(&rurge_str).expect("rurge.toml should parse as ModManifest");
    assert_eq!(mod_manifest.id, "emerald_craft");
    assert_eq!(mod_manifest.name, "Emerald Craft");

    // Check content.json
    let content_str = {
        let mut content_entry = archive.by_name("content.json").expect("content.json exists");
        let mut s = String::new();
        content_entry.read_to_string(&mut s).unwrap();
        s
    };
    let schema = ContentSchema::from_json_str(&content_str).expect("content.json valid");
    assert_eq!(schema.mod_id, "emerald_craft");
    assert_eq!(schema.blocks.len(), 1);
    assert_eq!(schema.blocks[0].id, "emerald_craft:emerald_lantern");
    assert_eq!(schema.items.len(), 1);
    assert_eq!(schema.items[0].id, "emerald_craft:emerald_chisel");
    assert_eq!(schema.recipes.len(), 1);

    // Check compatibility-report.json
    let comp_str = {
        let mut comp_entry = archive.by_name("compatibility-report.json").expect("compatibility-report exists");
        let mut s = String::new();
        comp_entry.read_to_string(&mut s).unwrap();
        s
    };
    assert!(comp_str.contains("\"status\": \"Full\""));

    // Check asset files
    assert!(archive.by_name("assets/emerald_craft/blockstates/emerald_lantern.json").is_ok());
    assert!(archive.by_name("assets/emerald_craft/models/block/emerald_lantern.json").is_ok());
    assert!(archive.by_name("assets/emerald_craft/models/item/emerald_chisel.json").is_ok());
    assert!(archive.by_name("assets/emerald_craft/textures/block/emerald_lantern.png").is_ok());
    assert!(archive.by_name("assets/emerald_craft/textures/item/emerald_chisel.png").is_ok());
    assert!(archive.by_name("assets/emerald_craft/lang/en_us.json").is_ok());
    assert!(archive.by_name("data/emerald_craft/recipes/emerald_chisel.json").is_ok());
}

#[test]
fn test_default_output_path_generation() {
    let temp = tempdir().expect("failed to create temp dir");
    let jar_path = temp.path().join("test_mod.jar");
    create_test_fabric_jar(&jar_path);

    let summary = convert_jar_to_rmod(&jar_path, None).expect("conversion should succeed");
    let expected_rmod = temp.path().join("emerald_craft-1.2.0.rmod");
    assert_eq!(summary.output_path, expected_rmod);
    assert!(expected_rmod.is_file());
}

#[test]
fn test_inspect_jar_without_output_file() {
    let temp = tempdir().expect("failed to create temp dir");
    let jar_path = temp.path().join("inspect_test.jar");
    create_test_fabric_jar(&jar_path);

    let inspection = inspect_jar(&jar_path).expect("inspection should succeed");
    assert_eq!(inspection.metadata.id, "emerald_craft");
    assert_eq!(inspection.blocks_count, 1);
    assert_eq!(inspection.items_count, 1);
    assert_eq!(inspection.recipes_count, 1);
    assert_eq!(inspection.block_ids, vec!["emerald_craft:emerald_lantern".to_string()]);
    assert_eq!(inspection.item_ids, vec!["emerald_craft:emerald_chisel".to_string()]);
    assert_eq!(inspection.compatibility.status, CompatibilityStatus::Full);

    // Verify no .rmod file was written
    let entries = std::fs::read_dir(temp.path()).unwrap().count();
    assert_eq!(entries, 1); // Only the original JAR exists
}

#[test]
fn test_error_handling_corrupt_jar_and_missing_file() {
    let temp = tempdir().expect("failed to create temp dir");

    // 1. Missing file
    let missing_path = temp.path().join("does_not_exist.jar");
    let res = convert_jar_to_rmod(&missing_path, None);
    assert!(res.is_err());

    // 2. Corrupt file
    let corrupt_path = temp.path().join("corrupt.jar");
    std::fs::write(&corrupt_path, b"not a valid zip file content").unwrap();
    let res = convert_jar_to_rmod(&corrupt_path, None);
    assert!(res.is_err());

    // 3. ZIP without manifest
    let no_manifest_path = temp.path().join("no_manifest.jar");
    let file = File::create(&no_manifest_path).unwrap();
    let mut zip = ZipWriter::new(file);
    zip.start_file("dummy.txt", SimpleFileOptions::default()).unwrap();
    zip.write_all(b"hello").unwrap();
    zip.finish().unwrap();

    let res = convert_jar_to_rmod(&no_manifest_path, None);
    assert!(res.is_err());
}

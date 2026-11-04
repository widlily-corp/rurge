use rurge_horizon::hooks::RurgeHorizonHooks;
use rurge_horizon::jar_scanner::convert_dropped_jars_in;
use rurge_runtime::engine::RurgeEngine;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
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
        let attr_len = (12 + bytecode.len()) as u32;
        code_attr.extend_from_slice(&attr_len.to_be_bytes());
        code_attr.extend_from_slice(&10u16.to_be_bytes()); // max_stack
        code_attr.extend_from_slice(&10u16.to_be_bytes()); // max_locals
        code_attr.extend_from_slice(&(bytecode.len() as u32).to_be_bytes());
        code_attr.extend_from_slice(bytecode);
        code_attr.extend_from_slice(&0u16.to_be_bytes()); // exception_table_length
        code_attr.extend_from_slice(&0u16.to_be_bytes()); // attributes_count

        let mut method = Vec::new();
        method.extend_from_slice(&0x0009u16.to_be_bytes()); // ACC_PUBLIC | ACC_STATIC
        method.extend_from_slice(&name_idx.to_be_bytes());
        method.extend_from_slice(&desc_idx.to_be_bytes());
        method.extend_from_slice(&1u16.to_be_bytes()); // 1 attribute (Code)
        method.extend_from_slice(&code_attr);

        self.methods.push(method);
    }

    fn build(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&0xCAFEBABEu32.to_be_bytes());
        buf.extend_from_slice(&0u16.to_be_bytes()); // minor_version
        buf.extend_from_slice(&65u16.to_be_bytes()); // major_version (Java 21)

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

fn create_test_fabric_jar(path: &Path) {
    let file = File::create(path).expect("failed to create test jar file");
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default();

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

    builder.add_method("init", "()V", &code);
    let class_bytes = builder.build();

    zip.start_file("com/example/emerald/EmeraldMod.class", options).unwrap();
    zip.write_all(&class_bytes).unwrap();

    // Models & textures
    let block_model = r#"{"parent": "block/cube_all", "textures": {"all": "emerald_craft:block/emerald_lantern"}}"#;
    zip.start_file("assets/emerald_craft/models/block/emerald_lantern.json", options).unwrap();
    zip.write_all(block_model.as_bytes()).unwrap();

    let item_model = r#"{"parent": "item/generated", "textures": {"layer0": "emerald_craft:item/emerald_chisel"}}"#;
    zip.start_file("assets/emerald_craft/models/item/emerald_chisel.json", options).unwrap();
    zip.write_all(item_model.as_bytes()).unwrap();

    let dummy_png = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    zip.start_file("assets/emerald_craft/textures/block/emerald_lantern.png", options).unwrap();
    zip.write_all(&dummy_png).unwrap();
    zip.start_file("assets/emerald_craft/textures/item/emerald_chisel.png", options).unwrap();
    zip.write_all(&dummy_png).unwrap();

    zip.finish().unwrap();
}

#[test]
fn test_convert_dropped_jars_empty_dir() {
    let tmp = tempdir().unwrap();
    let conversions = convert_dropped_jars_in(tmp.path());
    assert!(conversions.is_empty());
}

#[test]
fn test_convert_dropped_jars_and_caching() {
    let tmp = tempdir().unwrap();
    let mods_dir = tmp.path().join("mods");
    std::fs::create_dir_all(&mods_dir).unwrap();

    let jar_path = mods_dir.join("emerald_craft.jar");
    create_test_fabric_jar(&jar_path);

    // First run: converts the JAR to .rmod
    let conversions = convert_dropped_jars_in(&mods_dir);
    assert_eq!(conversions.len(), 1);
    let conv = &conversions[0];
    assert_eq!(conv.metadata.id, "emerald_craft");
    assert_eq!(conv.blocks_count, 1);
    assert_eq!(conv.items_count, 1);

    let expected_rmod = mods_dir.join("emerald_craft.rmod");
    assert!(expected_rmod.is_file(), "Generated .rmod must exist");

    // Second run: cached, skips conversion
    let second_run = convert_dropped_jars_in(&mods_dir);
    assert!(second_run.is_empty(), "Second run must skip up-to-date converted .rmod");
}

#[test]
fn test_ui_and_engine_integration_with_converted_mod() {
    let tmp = tempdir().unwrap();
    let mods_dir = tmp.path().join("mods");
    let cache_dir = tmp.path().join(".cache");
    std::fs::create_dir_all(&mods_dir).unwrap();

    let jar_path = mods_dir.join("emerald_craft.jar");
    create_test_fabric_jar(&jar_path);

    // 1. Auto-convert
    let conversions = convert_dropped_jars_in(&mods_dir);
    assert_eq!(conversions.len(), 1);

    // 2. Initialize engine
    let mut engine = RurgeEngine::with_dirs(&mods_dir, &cache_dir);
    engine.initialize().expect("Engine initialization should succeed");

    // 3. Verify content was loaded from the converted .rmod
    let content = engine.content();
    assert_eq!(content.block_count(), 1);
    assert_eq!(content.item_count(), 1);
    let block = content.get_block("emerald_craft:emerald_lantern");
    assert!(block.is_some());

    // 4. Verify in-game UI hook displays converted status
    let engine_arc = Arc::new(engine);
    let hooks = RurgeHorizonHooks::new(engine_arc);
    let mut native_mods = Vec::new();
    hooks.populate_native_mods(&mut native_mods);

    // Verify rurge_core description shows loaded custom content
    let core_mod = native_mods.iter().find(|m| m.id == "rurge_core").expect("rurge_core exists");
    assert!(core_mod.description.contains("custom blocks/items"));

    // Verify emerald_craft entry has [Converted JAR] tag
    let emerald_mod = native_mods
        .iter()
        .find(|m| m.id.contains("emerald_craft"))
        .expect("emerald_craft mod entry exists");
    assert!(
        emerald_mod.title.contains("[Converted JAR]"),
        "Title must include [Converted JAR] tag: {}",
        emerald_mod.title
    );
    assert!(
        emerald_mod.description.contains("Blocks: 1, Items: 1")
            || emerald_mod.description.contains("Blocks:")
    );
}

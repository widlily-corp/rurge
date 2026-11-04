use rurge_converter::bytecode::{ClassFile, ConstantPoolEntry};
use rurge_converter::error::ConverterError;

/// Helper struct for constructing synthetic JVM `.class` bytecode in tests.
struct SyntheticClassBuilder {
    minor_version: u16,
    major_version: u16,
    cp_entries: Vec<Vec<u8>>,
    this_class: u16,
    super_class: u16,
    interfaces: Vec<u16>,
    fields: Vec<Vec<u8>>,
    methods: Vec<Vec<u8>>,
    attributes: Vec<Vec<u8>>,
}

impl SyntheticClassBuilder {
    fn new(minor: u16, major: u16) -> Self {
        Self {
            minor_version: minor,
            major_version: major,
            cp_entries: Vec::new(),
            this_class: 0,
            super_class: 0,
            interfaces: Vec::new(),
            fields: Vec::new(),
            methods: Vec::new(),
            attributes: Vec::new(),
        }
    }

    fn add_utf8(&mut self, s: &str) -> u16 {
        let mut entry = Vec::new();
        entry.push(1); // CONSTANT_Utf8 tag
        let bytes = s.as_bytes();
        entry.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
        entry.extend_from_slice(bytes);
        self.cp_entries.push(entry);
        self.cp_entries.len() as u16
    }

    fn add_integer(&mut self, val: i32) -> u16 {
        let mut entry = Vec::new();
        entry.push(3); // CONSTANT_Integer
        entry.extend_from_slice(&val.to_be_bytes());
        self.cp_entries.push(entry);
        self.cp_entries.len() as u16
    }

    fn add_float(&mut self, val: f32) -> u16 {
        let mut entry = Vec::new();
        entry.push(4); // CONSTANT_Float
        entry.extend_from_slice(&val.to_be_bytes());
        self.cp_entries.push(entry);
        self.cp_entries.len() as u16
    }

    fn add_long(&mut self, val: i64) -> u16 {
        let mut entry = Vec::new();
        entry.push(5); // CONSTANT_Long
        entry.extend_from_slice(&val.to_be_bytes());
        self.cp_entries.push(entry);
        // Long takes two constant pool slots
        self.cp_entries.push(Vec::new()); // dummy entry
        (self.cp_entries.len() - 1) as u16
    }

    fn add_double(&mut self, val: f64) -> u16 {
        let mut entry = Vec::new();
        entry.push(6); // CONSTANT_Double
        entry.extend_from_slice(&val.to_be_bytes());
        self.cp_entries.push(entry);
        // Double takes two constant pool slots
        self.cp_entries.push(Vec::new()); // dummy entry
        (self.cp_entries.len() - 1) as u16
    }

    fn add_class(&mut self, name: &str) -> u16 {
        let name_idx = self.add_utf8(name);
        let mut entry = Vec::new();
        entry.push(7); // CONSTANT_Class tag
        entry.extend_from_slice(&name_idx.to_be_bytes());
        self.cp_entries.push(entry);
        self.cp_entries.len() as u16
    }

    fn add_string(&mut self, s: &str) -> u16 {
        let utf8_idx = self.add_utf8(s);
        let mut entry = Vec::new();
        entry.push(8); // CONSTANT_String tag
        entry.extend_from_slice(&utf8_idx.to_be_bytes());
        self.cp_entries.push(entry);
        self.cp_entries.len() as u16
    }

    fn add_name_and_type(&mut self, name: &str, descriptor: &str) -> u16 {
        let name_idx = self.add_utf8(name);
        let desc_idx = self.add_utf8(descriptor);
        let mut entry = Vec::new();
        entry.push(12); // CONSTANT_NameAndType
        entry.extend_from_slice(&name_idx.to_be_bytes());
        entry.extend_from_slice(&desc_idx.to_be_bytes());
        self.cp_entries.push(entry);
        self.cp_entries.len() as u16
    }

    fn add_method_ref(&mut self, class_name: &str, method_name: &str, descriptor: &str) -> u16 {
        let class_idx = self.add_class(class_name);
        let nt_idx = self.add_name_and_type(method_name, descriptor);
        let mut entry = Vec::new();
        entry.push(10); // CONSTANT_Methodref tag
        entry.extend_from_slice(&class_idx.to_be_bytes());
        entry.extend_from_slice(&nt_idx.to_be_bytes());
        self.cp_entries.push(entry);
        self.cp_entries.len() as u16
    }

    fn add_field_ref(&mut self, class_name: &str, field_name: &str, descriptor: &str) -> u16 {
        let class_idx = self.add_class(class_name);
        let nt_idx = self.add_name_and_type(field_name, descriptor);
        let mut entry = Vec::new();
        entry.push(9); // CONSTANT_Fieldref tag
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

        // Code attribute structure:
        // attribute_name_index: u16
        // attribute_length: u32
        // max_stack: u16
        // max_locals: u16
        // code_length: u32
        // code: [u8; code_length]
        // exception_table_length: u16 (0)
        // attributes_count: u16 (0)
        let mut code_attr = Vec::new();
        code_attr.extend_from_slice(&code_utf8_idx.to_be_bytes());
        let attr_length = (2 + 2 + 4 + bytecode.len() + 2 + 2) as u32;
        code_attr.extend_from_slice(&attr_length.to_be_bytes());
        code_attr.extend_from_slice(&4u16.to_be_bytes()); // max_stack
        code_attr.extend_from_slice(&2u16.to_be_bytes()); // max_locals
        code_attr.extend_from_slice(&(bytecode.len() as u32).to_be_bytes());
        code_attr.extend_from_slice(bytecode);
        code_attr.extend_from_slice(&0u16.to_be_bytes()); // exception_table_length
        code_attr.extend_from_slice(&0u16.to_be_bytes()); // attributes_count

        // Method structure:
        // access_flags: u16
        // name_index: u16
        // descriptor_index: u16
        // attributes_count: u16 (1: Code)
        let mut method = Vec::new();
        method.extend_from_slice(&0x0001u16.to_be_bytes()); // ACC_PUBLIC
        method.extend_from_slice(&name_idx.to_be_bytes());
        method.extend_from_slice(&desc_idx.to_be_bytes());
        method.extend_from_slice(&1u16.to_be_bytes()); // 1 attribute
        method.extend_from_slice(&code_attr);

        self.methods.push(method);
    }

    fn add_field(&mut self, name: &str, descriptor: &str) {
        let name_idx = self.add_utf8(name);
        let desc_idx = self.add_utf8(descriptor);
        let mut field = Vec::new();
        field.extend_from_slice(&0x0019u16.to_be_bytes()); // ACC_PUBLIC | ACC_STATIC | ACC_FINAL
        field.extend_from_slice(&name_idx.to_be_bytes());
        field.extend_from_slice(&desc_idx.to_be_bytes());
        field.extend_from_slice(&0u16.to_be_bytes()); // attributes_count
        self.fields.push(field);
    }

    fn build(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        // Magic
        bytes.extend_from_slice(&0xCAFEBABEu32.to_be_bytes());
        // Minor, Major version
        bytes.extend_from_slice(&self.minor_version.to_be_bytes());
        bytes.extend_from_slice(&self.major_version.to_be_bytes());

        // Constant pool count = entries + 1
        let cp_count = (self.cp_entries.len() + 1) as u16;
        bytes.extend_from_slice(&cp_count.to_be_bytes());
        for entry in &self.cp_entries {
            // Note: Long and Double push an empty dummy entry which outputs nothing
            bytes.extend_from_slice(entry);
        }

        // Access flags
        bytes.extend_from_slice(&0x0001u16.to_be_bytes()); // ACC_PUBLIC
        bytes.extend_from_slice(&self.this_class.to_be_bytes());
        bytes.extend_from_slice(&self.super_class.to_be_bytes());

        // Interfaces
        bytes.extend_from_slice(&(self.interfaces.len() as u16).to_be_bytes());
        for iface in &self.interfaces {
            bytes.extend_from_slice(&iface.to_be_bytes());
        }

        // Fields
        bytes.extend_from_slice(&(self.fields.len() as u16).to_be_bytes());
        for field in &self.fields {
            bytes.extend_from_slice(field);
        }

        // Methods
        bytes.extend_from_slice(&(self.methods.len() as u16).to_be_bytes());
        for method in &self.methods {
            bytes.extend_from_slice(method);
        }

        // Attributes
        bytes.extend_from_slice(&(self.attributes.len() as u16).to_be_bytes());
        for attr in &self.attributes {
            bytes.extend_from_slice(attr);
        }

        bytes
    }
}

#[test]
fn test_invalid_magic_returns_error() {
    let invalid_bytes = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x00, 0x00, 0x3D];
    let result = ClassFile::parse(&invalid_bytes);
    assert!(result.is_err());
    match result.unwrap_err() {
        ConverterError::InvalidMagic(magic) => assert_eq!(magic, 0xDEADBEEF),
        other => panic!("Expected InvalidMagic, got: {other:?}"),
    }
}

#[test]
fn test_truncated_bytes_returns_unexpected_eof() {
    let truncated = vec![0xCA, 0xFE];
    let result = ClassFile::parse(&truncated);
    assert!(result.is_err());
    match result.unwrap_err() {
        ConverterError::UnexpectedEof(_) => {}
        other => panic!("Expected UnexpectedEof, got: {other:?}"),
    }
}

#[test]
fn test_synthetic_minimal_class_parsing() {
    let mut builder = SyntheticClassBuilder::new(0, 61); // Java 17
    builder.set_classes("com/example/TestMod", "java/lang/Object");
    builder.add_field("MOD_ID", "Ljava/lang/String;");

    let bytes = builder.build();
    let classfile = ClassFile::parse(&bytes).expect("Failed to parse valid classfile");

    assert_eq!(classfile.major_version, 61);
    assert_eq!(classfile.minor_version, 0);
    assert_eq!(classfile.class_name(), "com/example/TestMod");
    assert_eq!(classfile.super_class_name(), Some("java/lang/Object"));

    assert!(classfile.references_class("com/example/TestMod"));
    assert!(classfile.references_class("com.example.TestMod")); // dot syntax normalized
    assert!(classfile.references_class("java/lang/Object"));
    assert!(!classfile.references_class("org/spongepowered/asm/mixin/Mixin"));

    assert_eq!(classfile.fields.len(), 1);
    assert_eq!(classfile.fields[0].name, "MOD_ID");
    assert_eq!(classfile.fields[0].descriptor, "Ljava/lang/String;");
}

#[test]
fn test_method_parsing_and_invocation_scanning() {
    let mut builder = SyntheticClassBuilder::new(0, 61);
    builder.set_classes("com/example/ModBlocks", "java/lang/Object");

    // Add string constant for block ID
    let string_idx = builder.add_string("copper_plus:fancy_block");

    // Add methodref to Registry.register(Registry, Identifier, Object)
    let methodref_idx = builder.add_method_ref(
        "net/minecraft/util/registry/Registry",
        "register",
        "(Lnet/minecraft/util/registry/Registry;Ljava/lang/String;Ljava/lang/Object;)Ljava/lang/Object;",
    );

    // Bytecode instructions:
    // 0x12, string_idx (ldc)
    // 0xB8, methodref_idx (invokestatic)
    // 0xB1 (return)
    let mut bytecode = Vec::new();
    bytecode.push(0x12); // ldc
    bytecode.push(string_idx as u8);
    bytecode.push(0xB8); // invokestatic
    bytecode.extend_from_slice(&methodref_idx.to_be_bytes());
    bytecode.push(0xB1); // return

    builder.add_method("registerAll", "()V", &bytecode);

    let bytes = builder.build();
    let classfile = ClassFile::parse(&bytes).expect("Failed to parse classfile with methods");

    assert_eq!(classfile.methods().len(), 1);
    let method = classfile.find_method("registerAll").expect("Method not found");
    assert_eq!(method.descriptor, "()V");

    let code = method.code.as_ref().expect("Code attribute missing");
    assert_eq!(code.bytecode.len(), bytecode.len());

    assert!(classfile.contains_string_constant("copper_plus:fancy_block"));
    assert!(!classfile.contains_string_constant("non_existent_block"));

    // Query invocations
    let invocations = classfile.find_method_invocations("net/minecraft/util/registry/Registry", "register");
    assert_eq!(invocations.len(), 1);
    let inv = &invocations[0];
    assert_eq!(inv.opcode, 0xB8);
    assert_eq!(inv.class_name, "net/minecraft/util/registry/Registry");
    assert_eq!(inv.method_name, "register");
    assert_eq!(inv.caller_method_name, "registerAll");
    assert_eq!(inv.offset, 2); // 0: ldc (2 bytes), 2: invokestatic

    // Wildcard query test
    let any_invocations = classfile.find_method_invocations("", "");
    assert_eq!(any_invocations.len(), 1);
}

#[test]
fn test_constant_pool_numerics_and_field_ref_resolution() {
    let mut builder = SyntheticClassBuilder::new(0, 61);
    builder.set_classes("com/example/Constants", "java/lang/Object");

    let int_idx = builder.add_integer(42);
    let float_idx = builder.add_float(2.718);
    let long_idx = builder.add_long(1234567890123456789i64);
    let double_idx = builder.add_double(3.141592653589793);
    let string_idx = builder.add_string("after_numbers");
    let fieldref_idx = builder.add_field_ref(
        "net/minecraft/world/level/block/Blocks",
        "STONE",
        "Lnet/minecraft/world/level/block/Block;",
    );

    let bytes = builder.build();
    let classfile = ClassFile::parse(&bytes).expect("Failed to parse classfile with constants");

    assert!(classfile.contains_string_constant("after_numbers"));
    assert_eq!(classfile.constant_pool.get_string(string_idx), Some("after_numbers"));

    // Verify integer and float
    match classfile.constant_pool.entry(int_idx) {
        Some(ConstantPoolEntry::Integer(val)) => assert_eq!(*val, 42),
        other => panic!("Expected Integer at index {int_idx}, got: {other:?}"),
    }

    match classfile.constant_pool.entry(float_idx) {
        Some(ConstantPoolEntry::Float(val)) => assert!((*val - 2.718).abs() < 1e-5),
        other => panic!("Expected Float at index {float_idx}, got: {other:?}"),
    }

    // Verify long and double
    match classfile.constant_pool.entry(long_idx) {
        Some(ConstantPoolEntry::Long(val)) => assert_eq!(*val, 1234567890123456789i64),
        other => panic!("Expected Long at index {long_idx}, got: {other:?}"),
    }

    match classfile.constant_pool.entry(double_idx) {
        Some(ConstantPoolEntry::Double(val)) => assert!((*val - 3.141592653589793).abs() < 1e-10),
        other => panic!("Expected Double at index {double_idx}, got: {other:?}"),
    }

    // Verify field reference resolution
    let (cls, field_name, desc) = classfile
        .constant_pool
        .resolve_field_ref(fieldref_idx)
        .expect("Failed to resolve field ref");
    assert_eq!(cls, "net/minecraft/world/level/block/Blocks");
    assert_eq!(field_name, "STONE");
    assert_eq!(desc, "Lnet/minecraft/world/level/block/Block;");
}

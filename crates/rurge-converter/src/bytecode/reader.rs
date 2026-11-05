use crate::bytecode::constant_pool::{ConstantPool, ConstantPoolEntry};
use crate::error::{ConverterError, Result};
use std::io::Read;

/// Maximum allowable decompressed size for a single classfile or bytecode buffer (64 MB).
///
/// Guards against decompression bombs (zip bombs) and out-of-memory denial-of-service.
pub const MAX_CLASSFILE_SIZE: usize = 64 * 1024 * 1024;

/// Attribute metadata attached to a class, field, or method.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttributeInfo {
    pub name_index: u16,
    pub name: String,
    pub data: Vec<u8>,
}

/// Information about a field declared in a JVM class.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldInfo {
    pub access_flags: u16,
    pub name_index: u16,
    pub name: String,
    pub descriptor_index: u16,
    pub descriptor: String,
    pub attributes: Vec<AttributeInfo>,
}

/// Information about a method declared in a JVM class.
#[derive(Debug, Clone, PartialEq)]
pub struct MethodInfo {
    pub access_flags: u16,
    pub name_index: u16,
    pub name: String,
    pub descriptor_index: u16,
    pub descriptor: String,
    pub code: Option<CodeInfo>,
    pub attributes: Vec<AttributeInfo>,
}

/// Raw bytecode instructions and execution stack constraints of a method.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeInfo {
    pub max_stack: u16,
    pub max_locals: u16,
    pub bytecode: Vec<u8>,
}

/// A detected method invocation instruction inside method bytecode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodInvocation {
    /// JVM opcode (0xB6: invokevirtual, 0xB7: invokespecial, 0xB8: invokestatic, 0xB9: invokeinterface)
    pub opcode: u8,
    /// Target class name (e.g. `"net/minecraft/util/registry/Registry"`)
    pub class_name: String,
    /// Target method name (e.g. `"register"`)
    pub method_name: String,
    /// Method descriptor (e.g. `"(Lnet/minecraft/util/registry/Registry;Ljava/lang/String;)Ljava/lang/Object;"`)
    pub descriptor: String,
    /// Byte offset of the invoke instruction inside the method's bytecode
    pub offset: usize,
    /// Name of the caller method in the enclosing class (e.g. `"<clinit>"`, `"onInitialize"`)
    pub caller_method_name: String,
}

/// A parsed JVM `.class` binary structure.
#[derive(Debug, Clone)]
pub struct ClassFile {
    pub minor_version: u16,
    pub major_version: u16,
    pub constant_pool: ConstantPool,
    pub access_flags: u16,
    pub this_class: u16,
    pub super_class: u16,
    pub interfaces: Vec<u16>,
    pub fields: Vec<FieldInfo>,
    pub methods: Vec<MethodInfo>,
    pub attributes: Vec<AttributeInfo>,
}

impl ClassFile {
    /// Parses a JVM classfile from a raw byte slice.
    ///
    /// Validates magic `0xCAFEBABE` and enforces `MAX_CLASSFILE_SIZE` (64 MB) limit
    /// against decompression bombs and memory exhaustion attacks.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_CLASSFILE_SIZE {
            return Err(ConverterError::Other(format!(
                "Classfile size ({} bytes) exceeds maximum allowable limit of 64MB (decompression bomb protection)",
                bytes.len()
            )));
        }

        let mut cursor = ByteCursor::new(bytes);

        // Header magic 0xCAFEBABE
        let magic = cursor.read_u32()?;
        if magic != 0xCAFEBABE {
            return Err(ConverterError::InvalidMagic(magic));
        }

        let minor_version = cursor.read_u16()?;
        let major_version = cursor.read_u16()?;

        // Constant pool
        let cp_count = cursor.read_u16()?;
        let mut cp_entries = Vec::with_capacity(cp_count as usize);
        cp_entries.push(ConstantPoolEntry::Empty); // 1-based indexing, slot 0 is empty

        let mut idx = 1;
        while idx < cp_count {
            let tag = cursor.read_u8()?;
            match tag {
                1 => {
                    // CONSTANT_Utf8
                    let len = cursor.read_u16()? as usize;
                    let slice = cursor.read_bytes(len)?;
                    let s = match std::str::from_utf8(slice) {
                        Ok(v) => v.to_string(),
                        Err(_) => String::from_utf8_lossy(slice).into_owned(),
                    };
                    cp_entries.push(ConstantPoolEntry::Utf8(s));
                    idx += 1;
                }
                3 => {
                    // CONSTANT_Integer
                    let val = cursor.read_i32()?;
                    cp_entries.push(ConstantPoolEntry::Integer(val));
                    idx += 1;
                }
                4 => {
                    // CONSTANT_Float
                    let val = cursor.read_f32()?;
                    cp_entries.push(ConstantPoolEntry::Float(val));
                    idx += 1;
                }
                5 => {
                    // CONSTANT_Long (occupies 2 pool entries)
                    let val = cursor.read_i64()?;
                    cp_entries.push(ConstantPoolEntry::Long(val));
                    cp_entries.push(ConstantPoolEntry::Empty);
                    idx += 2;
                }
                6 => {
                    // CONSTANT_Double (occupies 2 pool entries)
                    let val = cursor.read_f64()?;
                    cp_entries.push(ConstantPoolEntry::Double(val));
                    cp_entries.push(ConstantPoolEntry::Empty);
                    idx += 2;
                }
                7 => {
                    // CONSTANT_Class
                    let name_index = cursor.read_u16()?;
                    cp_entries.push(ConstantPoolEntry::Class { name_index });
                    idx += 1;
                }
                8 => {
                    // CONSTANT_String
                    let string_index = cursor.read_u16()?;
                    cp_entries.push(ConstantPoolEntry::StringRef { string_index });
                    idx += 1;
                }
                9 => {
                    // CONSTANT_Fieldref
                    let class_index = cursor.read_u16()?;
                    let name_and_type_index = cursor.read_u16()?;
                    cp_entries.push(ConstantPoolEntry::Fieldref {
                        class_index,
                        name_and_type_index,
                    });
                    idx += 1;
                }
                10 => {
                    // CONSTANT_Methodref
                    let class_index = cursor.read_u16()?;
                    let name_and_type_index = cursor.read_u16()?;
                    cp_entries.push(ConstantPoolEntry::Methodref {
                        class_index,
                        name_and_type_index,
                    });
                    idx += 1;
                }
                11 => {
                    // CONSTANT_InterfaceMethodref
                    let class_index = cursor.read_u16()?;
                    let name_and_type_index = cursor.read_u16()?;
                    cp_entries.push(ConstantPoolEntry::InterfaceMethodref {
                        class_index,
                        name_and_type_index,
                    });
                    idx += 1;
                }
                12 => {
                    // CONSTANT_NameAndType
                    let name_index = cursor.read_u16()?;
                    let descriptor_index = cursor.read_u16()?;
                    cp_entries.push(ConstantPoolEntry::NameAndType {
                        name_index,
                        descriptor_index,
                    });
                    idx += 1;
                }
                15 => {
                    // CONSTANT_MethodHandle
                    let reference_kind = cursor.read_u8()?;
                    let reference_index = cursor.read_u16()?;
                    cp_entries.push(ConstantPoolEntry::MethodHandle {
                        reference_kind,
                        reference_index,
                    });
                    idx += 1;
                }
                16 => {
                    // CONSTANT_MethodType
                    let descriptor_index = cursor.read_u16()?;
                    cp_entries.push(ConstantPoolEntry::MethodType { descriptor_index });
                    idx += 1;
                }
                17 => {
                    // CONSTANT_Dynamic
                    let bootstrap_method_attr_index = cursor.read_u16()?;
                    let name_and_type_index = cursor.read_u16()?;
                    cp_entries.push(ConstantPoolEntry::Dynamic {
                        bootstrap_method_attr_index,
                        name_and_type_index,
                    });
                    idx += 1;
                }
                18 => {
                    // CONSTANT_InvokeDynamic
                    let bootstrap_method_attr_index = cursor.read_u16()?;
                    let name_and_type_index = cursor.read_u16()?;
                    cp_entries.push(ConstantPoolEntry::InvokeDynamic {
                        bootstrap_method_attr_index,
                        name_and_type_index,
                    });
                    idx += 1;
                }
                19 => {
                    // CONSTANT_Module
                    let name_index = cursor.read_u16()?;
                    cp_entries.push(ConstantPoolEntry::Module { name_index });
                    idx += 1;
                }
                20 => {
                    // CONSTANT_Package
                    let name_index = cursor.read_u16()?;
                    cp_entries.push(ConstantPoolEntry::Package { name_index });
                    idx += 1;
                }
                other => return Err(ConverterError::InvalidConstantTag(other)),
            }
        }

        let constant_pool = ConstantPool::from_entries(cp_entries);

        let access_flags = cursor.read_u16()?;
        let this_class = cursor.read_u16()?;
        let super_class = cursor.read_u16()?;

        // Interfaces
        let interfaces_count = cursor.read_u16()?;
        let mut interfaces = Vec::with_capacity(interfaces_count as usize);
        for _ in 0..interfaces_count {
            interfaces.push(cursor.read_u16()?);
        }

        // Fields
        let fields_count = cursor.read_u16()?;
        let mut fields = Vec::with_capacity(fields_count as usize);
        for _ in 0..fields_count {
            let f_access = cursor.read_u16()?;
            let f_name_idx = cursor.read_u16()?;
            let f_desc_idx = cursor.read_u16()?;
            let f_name = constant_pool.get_utf8(f_name_idx).unwrap_or("").to_string();
            let f_desc = constant_pool.get_utf8(f_desc_idx).unwrap_or("").to_string();
            let f_attrs = Self::read_attributes(&mut cursor, &constant_pool)?;
            fields.push(FieldInfo {
                access_flags: f_access,
                name_index: f_name_idx,
                name: f_name,
                descriptor_index: f_desc_idx,
                descriptor: f_desc,
                attributes: f_attrs,
            });
        }

        // Methods
        let methods_count = cursor.read_u16()?;
        let mut methods = Vec::with_capacity(methods_count as usize);
        for _ in 0..methods_count {
            let m_access = cursor.read_u16()?;
            let m_name_idx = cursor.read_u16()?;
            let m_desc_idx = cursor.read_u16()?;
            let m_name = constant_pool.get_utf8(m_name_idx).unwrap_or("").to_string();
            let m_desc = constant_pool.get_utf8(m_desc_idx).unwrap_or("").to_string();
            let m_attrs = Self::read_attributes(&mut cursor, &constant_pool)?;

            // Extract Code attribute if present
            let code = m_attrs
                .iter()
                .find(|a| a.name == "Code")
                .and_then(|a| Self::parse_code_attribute(&a.data));

            methods.push(MethodInfo {
                access_flags: m_access,
                name_index: m_name_idx,
                name: m_name,
                descriptor_index: m_desc_idx,
                descriptor: m_desc,
                code,
                attributes: m_attrs,
            });
        }

        // Class attributes
        let attributes = Self::read_attributes(&mut cursor, &constant_pool)?;

        Ok(Self {
            minor_version,
            major_version,
            constant_pool,
            access_flags,
            this_class,
            super_class,
            interfaces,
            fields,
            methods,
            attributes,
        })
    }

    fn read_attributes(
        cursor: &mut ByteCursor<'_>,
        constant_pool: &ConstantPool,
    ) -> Result<Vec<AttributeInfo>> {
        let count = cursor.read_u16()?;
        let mut attributes = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let name_index = cursor.read_u16()?;
            let length = cursor.read_u32()? as usize;
            let data = cursor.read_vec(length)?;
            let name = constant_pool.get_utf8(name_index).unwrap_or("").to_string();
            attributes.push(AttributeInfo {
                name_index,
                name,
                data,
            });
        }
        Ok(attributes)
    }

    fn parse_code_attribute(data: &[u8]) -> Option<CodeInfo> {
        if data.len() < 8 {
            return None;
        }
        let max_stack = u16::from_be_bytes([data[0], data[1]]);
        let max_locals = u16::from_be_bytes([data[2], data[3]]);
        let code_len = u32::from_be_bytes([data[4], data[5], data[6], data[7]]) as usize;
        if code_len > MAX_CLASSFILE_SIZE || code_len > data.len() - 8 {
            return None;
        }
        let bytecode = data[8..8 + code_len].to_vec();
        Some(CodeInfo {
            max_stack,
            max_locals,
            bytecode,
        })
    }

    /// Returns the fully qualified binary name of this class (e.g. `"com/example/MyMod"`).
    pub fn class_name(&self) -> &str {
        self.constant_pool.get_class_name(self.this_class).unwrap_or("")
    }

    /// Returns the fully qualified binary name of this class's superclass, or `None` if `Object`.
    pub fn super_class_name(&self) -> Option<&str> {
        if self.super_class == 0 {
            None
        } else {
            self.constant_pool.get_class_name(self.super_class)
        }
    }

    /// Returns a slice of all declared methods.
    pub fn methods(&self) -> &[MethodInfo] {
        &self.methods
    }

    /// Finds a method declared in this class by name.
    pub fn find_method(&self, name: &str) -> Option<&MethodInfo> {
        self.methods.iter().find(|m| m.name == name)
    }

    /// Checks whether the constant pool contains a string constant matching `s`.
    pub fn contains_string_constant(&self, s: &str) -> bool {
        for (_, entry) in self.constant_pool.iter() {
            match entry {
                ConstantPoolEntry::StringRef { string_index }
                    if self.constant_pool.get_utf8(*string_index) == Some(s) =>
                {
                    return true;
                }
                _ => {}
            }
        }
        false
    }

    /// Checks whether the class references another class (in constant pool class entries).
    /// Accepts both dot notation (`"com.example.Mod"`) and slash notation (`"com/example/Mod"`).
    pub fn references_class(&self, class_name: &str) -> bool {
        let norm = class_name.replace('.', "/");
        for (_, entry) in self.constant_pool.iter() {
            match entry {
                ConstantPoolEntry::Class { name_index }
                    if self.constant_pool.get_utf8(*name_index) == Some(norm.as_str()) =>
                {
                    return true;
                }
                _ => {}
            }
        }
        false
    }

    /// Scans bytecode across all methods in this class for invocations of `target_class` and `target_method`.
    /// Pass an empty string for `target_class` or `target_method` to wildcard match.
    pub fn find_method_invocations(
        &self,
        target_class: &str,
        target_method: &str,
    ) -> Vec<MethodInvocation> {
        let mut invocations = Vec::new();
        let norm_target_class = target_class.replace('.', "/");

        for method in &self.methods {
            if let Some(code_info) = &method.code {
                let code = &code_info.bytecode;
                let mut offset = 0;
                while offset < code.len() {
                    let opcode = code[offset];
                    let len = instruction_length(code, offset);

                    let cp_index = match opcode {
                        0xB6..=0xB9 => {
                            // invokevirtual, invokespecial, invokestatic, invokeinterface
                            if offset + 2 < code.len() {
                                Some(u16::from_be_bytes([code[offset + 1], code[offset + 2]]))
                            } else {
                                None
                            }
                        }
                        _ => None,
                    };

                    if let Some((cls, mth, desc)) =
                        cp_index.and_then(|idx| self.constant_pool.resolve_method_ref(idx))
                    {
                        let class_matches =
                            norm_target_class.is_empty() || cls == norm_target_class;
                        let method_matches =
                            target_method.is_empty() || mth == target_method;
                        if class_matches && method_matches {
                            invocations.push(MethodInvocation {
                                opcode,
                                class_name: cls.to_string(),
                                method_name: mth.to_string(),
                                descriptor: desc.to_string(),
                                offset,
                                caller_method_name: method.name.clone(),
                            });
                        }
                    }

                    offset += if len > 0 { len } else { 1 };
                }
            }
        }

        invocations
    }
}

/// Computes the exact length in bytes of a JVM instruction starting at `offset`.
pub fn instruction_length(code: &[u8], offset: usize) -> usize {
    if offset >= code.len() {
        return 0;
    }
    let opcode = code[offset];
    match opcode {
        // 1-byte opcodes (no operands):
        0x00..=0x0F // nop, aconst_null, iconst_*, lconst_*, fconst_*, dconst_*
        | 0x1A..=0x35 // iload_*, lload_*, fload_*, dload_*, aload_*, [i..s]aload
        | 0x3B..=0x56 // istore_*, lstore_*, fstore_*, dstore_*, astore_*, [i..s]astore
        | 0x57..=0x83 // pop, dup*, swap, math operations
        | 0x85..=0x98 // numeric conversions, cmp*
        | 0xAC..=0xB1 // [i..a]return, return
        | 0xBE | 0xBF | 0xC2 | 0xC3 // arraylength, athrow, monitorenter, monitorexit
        => 1,

        // 2-byte opcodes (opcode + 1 byte):
        0x10 // bipush
        | 0x12 // ldc
        | 0x15..=0x19 // [i..a]load index
        | 0x36..=0x3A // [i..a]store index
        | 0xA9 // ret
        | 0xBC // newarray
        => 2,

        // 3-byte opcodes (opcode + 2 bytes):
        0x11 // sipush
        | 0x13 // ldc_w
        | 0x14 // ldc2_w
        | 0x84 // iinc index, const
        | 0x99..=0xA8 // if*, if_icmp*, if_acmp*, goto, jsr
        | 0xB2..=0xB8 // getstatic, putstatic, getfield, putfield, invokevirtual, invokespecial, invokestatic
        | 0xBB // new
        | 0xBD // anewarray
        | 0xC0 // checkcast
        | 0xC1 // instanceof
        | 0xC6 // ifnull
        | 0xC7 // ifnonnull
        => 3,

        // 4-byte opcodes:
        0xC5 // multianewarray
        => 4,

        // 5-byte opcodes:
        0xB9 // invokeinterface
        | 0xBA // invokedynamic
        | 0xC8 // goto_w
        | 0xC9 // jsr_w
        => 5,

        // wide instruction
        0xC4 => {
            if offset + 1 < code.len() {
                let sub = code[offset + 1];
                if sub == 0x84 {
                    6 // wide iinc
                } else {
                    4 // wide <load/store>
                }
            } else {
                1
            }
        }

        // tableswitch
        0xAA => {
            let pad = (4 - ((offset + 1) % 4)) % 4;
            let base = offset + 1 + pad;
            if base + 12 <= code.len() {
                let low = i32::from_be_bytes([code[base + 4], code[base + 5], code[base + 6], code[base + 7]]);
                let high = i32::from_be_bytes([code[base + 8], code[base + 9], code[base + 10], code[base + 11]]);
                let count = if high >= low {
                    let diff = (high as i64) - (low as i64) + 1;
                    diff.max(0) as usize
                } else {
                    0
                };
                1 + pad + 12 + count.saturating_mul(4)
            } else {
                1
            }
        }

        // lookupswitch
        0xAB => {
            let pad = (4 - ((offset + 1) % 4)) % 4;
            let base = offset + 1 + pad;
            if base + 8 <= code.len() {
                let npairs = i32::from_be_bytes([code[base + 4], code[base + 5], code[base + 6], code[base + 7]]);
                let count = if npairs > 0 { npairs as usize } else { 0 };
                1 + pad + 8 + count.saturating_mul(8)
            } else {
                1
            }
        }

        _ => 1,
    }
}

/// Helper cursor for sequential binary decoding.
struct ByteCursor<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> ByteCursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    fn read_u8(&mut self) -> Result<u8> {
        if self.offset < self.data.len() {
            let val = self.data[self.offset];
            self.offset += 1;
            Ok(val)
        } else {
            Err(ConverterError::UnexpectedEof(self.offset))
        }
    }

    fn read_u16(&mut self) -> Result<u16> {
        if self.offset + 2 <= self.data.len() {
            let val = u16::from_be_bytes([self.data[self.offset], self.data[self.offset + 1]]);
            self.offset += 2;
            Ok(val)
        } else {
            Err(ConverterError::UnexpectedEof(self.offset))
        }
    }

    fn read_u32(&mut self) -> Result<u32> {
        if self.offset + 4 <= self.data.len() {
            let val = u32::from_be_bytes([
                self.data[self.offset],
                self.data[self.offset + 1],
                self.data[self.offset + 2],
                self.data[self.offset + 3],
            ]);
            self.offset += 4;
            Ok(val)
        } else {
            Err(ConverterError::UnexpectedEof(self.offset))
        }
    }

    fn read_i32(&mut self) -> Result<i32> {
        if self.offset + 4 <= self.data.len() {
            let val = i32::from_be_bytes([
                self.data[self.offset],
                self.data[self.offset + 1],
                self.data[self.offset + 2],
                self.data[self.offset + 3],
            ]);
            self.offset += 4;
            Ok(val)
        } else {
            Err(ConverterError::UnexpectedEof(self.offset))
        }
    }

    fn read_f32(&mut self) -> Result<f32> {
        if self.offset + 4 <= self.data.len() {
            let val = f32::from_be_bytes([
                self.data[self.offset],
                self.data[self.offset + 1],
                self.data[self.offset + 2],
                self.data[self.offset + 3],
            ]);
            self.offset += 4;
            Ok(val)
        } else {
            Err(ConverterError::UnexpectedEof(self.offset))
        }
    }

    fn read_i64(&mut self) -> Result<i64> {
        if self.offset + 8 <= self.data.len() {
            let val = i64::from_be_bytes([
                self.data[self.offset],
                self.data[self.offset + 1],
                self.data[self.offset + 2],
                self.data[self.offset + 3],
                self.data[self.offset + 4],
                self.data[self.offset + 5],
                self.data[self.offset + 6],
                self.data[self.offset + 7],
            ]);
            self.offset += 8;
            Ok(val)
        } else {
            Err(ConverterError::UnexpectedEof(self.offset))
        }
    }

    fn read_f64(&mut self) -> Result<f64> {
        if self.offset + 8 <= self.data.len() {
            let val = f64::from_be_bytes([
                self.data[self.offset],
                self.data[self.offset + 1],
                self.data[self.offset + 2],
                self.data[self.offset + 3],
                self.data[self.offset + 4],
                self.data[self.offset + 5],
                self.data[self.offset + 6],
                self.data[self.offset + 7],
            ]);
            self.offset += 8;
            Ok(val)
        } else {
            Err(ConverterError::UnexpectedEof(self.offset))
        }
    }

    fn read_bytes(&mut self, len: usize) -> Result<&'a [u8]> {
        if len > MAX_CLASSFILE_SIZE {
            return Err(ConverterError::Other(format!(
                "Requested read length ({len} bytes) exceeds safety limit of 64MB"
            )));
        }
        if self.offset + len <= self.data.len() {
            let slice = &self.data[self.offset..self.offset + len];
            self.offset += len;
            Ok(slice)
        } else {
            Err(ConverterError::UnexpectedEof(self.offset))
        }
    }

    fn read_vec(&mut self, len: usize) -> Result<Vec<u8>> {
        self.read_bytes(len).map(|s| s.to_vec())
    }
}

/// Safely reads decompressed bytes from an input stream up to `MAX_CLASSFILE_SIZE` (64 MB).
///
/// Prevents zip decompression bombs and out-of-memory denial of service.
pub fn read_bounded_classfile<R: Read>(reader: &mut R) -> Result<Vec<u8>> {
    read_bounded_stream(reader, MAX_CLASSFILE_SIZE as u64)
}

/// Reads bytes from a reader up to `limit` bytes.
///
/// Returns `ConverterError::Other` if the stream exceeds the limit.
pub fn read_bounded_stream<R: Read>(reader: &mut R, limit: u64) -> Result<Vec<u8>> {
    let mut take = Read::take(reader, limit + 1);
    let mut buf = Vec::new();
    take.read_to_end(&mut buf)?;
    if buf.len() as u64 > limit {
        return Err(ConverterError::Other(format!(
            "Decompressed stream exceeds maximum size limit of {limit} bytes (decompression bomb protection)"
        )));
    }
    Ok(buf)
}

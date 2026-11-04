/// A single entry in the JVM constant pool table.
#[derive(Debug, Clone, PartialEq)]
pub enum ConstantPoolEntry {
    /// Slot 0 or unusable trailing slot for 8-byte constants (Long/Double).
    Empty,
    /// CONSTANT_Utf8 (tag 1)
    Utf8(String),
    /// CONSTANT_Integer (tag 3)
    Integer(i32),
    /// CONSTANT_Float (tag 4)
    Float(f32),
    /// CONSTANT_Long (tag 5, occupies 2 constant pool slots)
    Long(i64),
    /// CONSTANT_Double (tag 6, occupies 2 constant pool slots)
    Double(f64),
    /// CONSTANT_Class (tag 7)
    Class {
        name_index: u16,
    },
    /// CONSTANT_String (tag 8)
    StringRef {
        string_index: u16,
    },
    /// CONSTANT_Fieldref (tag 9)
    Fieldref {
        class_index: u16,
        name_and_type_index: u16,
    },
    /// CONSTANT_Methodref (tag 10)
    Methodref {
        class_index: u16,
        name_and_type_index: u16,
    },
    /// CONSTANT_InterfaceMethodref (tag 11)
    InterfaceMethodref {
        class_index: u16,
        name_and_type_index: u16,
    },
    /// CONSTANT_NameAndType (tag 12)
    NameAndType {
        name_index: u16,
        descriptor_index: u16,
    },
    /// CONSTANT_MethodHandle (tag 15)
    MethodHandle {
        reference_kind: u8,
        reference_index: u16,
    },
    /// CONSTANT_MethodType (tag 16)
    MethodType {
        descriptor_index: u16,
    },
    /// CONSTANT_Dynamic (tag 17)
    Dynamic {
        bootstrap_method_attr_index: u16,
        name_and_type_index: u16,
    },
    /// CONSTANT_InvokeDynamic (tag 18)
    InvokeDynamic {
        bootstrap_method_attr_index: u16,
        name_and_type_index: u16,
    },
    /// CONSTANT_Module (tag 19)
    Module {
        name_index: u16,
    },
    /// CONSTANT_Package (tag 20)
    Package {
        name_index: u16,
    },
}

/// The constant pool of a compiled Java classfile.
///
/// JVM constant pool indices are 1-based. Slot 0 is represented as [`ConstantPoolEntry::Empty`].
#[derive(Debug, Clone, Default)]
pub struct ConstantPool {
    entries: Vec<ConstantPoolEntry>,
}

impl ConstantPool {
    /// Creates an empty constant pool with slot 0 initialized to `Empty`.
    pub fn new() -> Self {
        Self {
            entries: vec![ConstantPoolEntry::Empty],
        }
    }

    /// Creates a constant pool from pre-allocated entries.
    pub fn from_entries(entries: Vec<ConstantPoolEntry>) -> Self {
        Self { entries }
    }

    /// Appends an entry to the constant pool.
    pub fn push(&mut self, entry: ConstantPoolEntry) {
        self.entries.push(entry);
    }

    /// Retrieves an entry by its 1-based index.
    pub fn entry(&self, index: u16) -> Option<&ConstantPoolEntry> {
        let idx = index as usize;
        match self.entries.get(idx) {
            Some(ConstantPoolEntry::Empty) | None => None,
            Some(entry) => Some(entry),
        }
    }

    /// Total entries in the pool including empty slots.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` if the constant pool contains no usable entries.
    pub fn is_empty(&self) -> bool {
        self.entries.len() <= 1
    }

    /// Resolves a `CONSTANT_Utf8` string at the given index.
    pub fn get_utf8(&self, index: u16) -> Option<&str> {
        match self.entry(index)? {
            ConstantPoolEntry::Utf8(s) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Resolves a class name from a `CONSTANT_Class` entry at the given index.
    pub fn get_class_name(&self, index: u16) -> Option<&str> {
        match self.entry(index)? {
            ConstantPoolEntry::Class { name_index } => self.get_utf8(*name_index),
            _ => None,
        }
    }

    /// Resolves a string value from a `CONSTANT_String` entry at the given index.
    pub fn get_string(&self, index: u16) -> Option<&str> {
        match self.entry(index)? {
            ConstantPoolEntry::StringRef { string_index } => self.get_utf8(*string_index),
            _ => None,
        }
    }

    /// Resolves a method reference (either standard or interface) into:
    /// `(class_name, method_name, descriptor)`.
    pub fn resolve_method_ref(&self, index: u16) -> Option<(&str, &str, &str)> {
        let (class_idx, nt_idx) = match self.entry(index)? {
            ConstantPoolEntry::Methodref {
                class_index,
                name_and_type_index,
            }
            | ConstantPoolEntry::InterfaceMethodref {
                class_index,
                name_and_type_index,
            } => (*class_index, *name_and_type_index),
            _ => return None,
        };

        let class_name = self.get_class_name(class_idx)?;

        match self.entry(nt_idx)? {
            ConstantPoolEntry::NameAndType {
                name_index,
                descriptor_index,
            } => {
                let method_name = self.get_utf8(*name_index)?;
                let descriptor = self.get_utf8(*descriptor_index)?;
                Some((class_name, method_name, descriptor))
            }
            _ => None,
        }
    }

    /// Resolves a field reference into:
    /// `(class_name, field_name, descriptor)`.
    pub fn resolve_field_ref(&self, index: u16) -> Option<(&str, &str, &str)> {
        let (class_idx, nt_idx) = match self.entry(index)? {
            ConstantPoolEntry::Fieldref {
                class_index,
                name_and_type_index,
            } => (*class_index, *name_and_type_index),
            _ => return None,
        };

        let class_name = self.get_class_name(class_idx)?;

        match self.entry(nt_idx)? {
            ConstantPoolEntry::NameAndType {
                name_index,
                descriptor_index,
            } => {
                let field_name = self.get_utf8(*name_index)?;
                let descriptor = self.get_utf8(*descriptor_index)?;
                Some((class_name, field_name, descriptor))
            }
            _ => None,
        }
    }

    /// Iterates over all usable entries in the pool along with their 1-based index.
    pub fn iter(&self) -> impl Iterator<Item = (u16, &ConstantPoolEntry)> {
        self.entries
            .iter()
            .enumerate()
            .filter_map(|(idx, entry)| match entry {
                ConstantPoolEntry::Empty => None,
                _ => Some((idx as u16, entry)),
            })
    }
}

//! Rurge Smart JAR-to-RMOD Converter.
//!
//! Provides zero-JVM static analysis, JVM classfile parsing, multi-loader pattern matching,
//! and asset normalization to convert legacy Minecraft Java mods into native declarative `.rmod` packages.

pub mod bytecode;
pub mod error;
pub mod ir;
pub mod manifest;
pub mod patterns;

pub use bytecode::{
    instruction_length, AttributeInfo, ClassFile, CodeInfo, ConstantPool, ConstantPoolEntry,
    FieldInfo, MethodInfo, MethodInvocation,
};
pub use error::{ConverterError, Result};
pub use ir::{BlockDef, ContentIR, FoodStats, ItemDef, RecipeDef, ToolStats, ToolType};
pub use manifest::{
    parse_fabric_manifest, parse_forge_manifest, parse_legacy_manifest, parse_manifest_from_zip,
    ModLoader, ModMetadata,
};
pub use patterns::scan_classes_for_content;

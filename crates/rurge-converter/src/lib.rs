//! Rurge Smart JAR-to-RMOD Converter.
//!
//! Provides zero-JVM static analysis, JVM classfile parsing, multi-loader pattern matching,
//! and asset normalization to convert legacy Minecraft Java mods into native declarative `.rmod` packages.

pub mod bytecode;
pub mod error;

pub use bytecode::{
    instruction_length, AttributeInfo, ClassFile, CodeInfo, ConstantPool, ConstantPoolEntry,
    FieldInfo, MethodInfo, MethodInvocation,
};
pub use error::{ConverterError, Result};

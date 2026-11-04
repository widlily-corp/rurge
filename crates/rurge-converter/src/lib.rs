//! Rurge Smart JAR-to-RMOD Converter.
//!
//! Provides zero-JVM static analysis, JVM classfile parsing, multi-loader pattern matching,
//! and asset normalization to convert legacy Minecraft Java mods into native declarative `.rmod` packages.

pub mod assets;
pub mod bytecode;
pub mod compatibility;
pub mod error;
pub mod ir;
pub mod manifest;
pub mod patterns;
pub mod schema;

pub use assets::{
    extract_assets_from_zip, parse_lang_json, parse_legacy_lang, parse_recipe_json,
    resolve_model_parents, ExtractedAssets,
};
pub use bytecode::{
    instruction_length, AttributeInfo, ClassFile, CodeInfo, ConstantPool, ConstantPoolEntry,
    FieldInfo, MethodInfo, MethodInvocation,
};
pub use compatibility::{audit_compatibility, CompatibilityReport, CompatibilityStatus};
pub use error::{ConverterError, Result};
pub use ir::{BlockDef, ContentIR, FoodStats, ItemDef, RecipeDef, ToolStats, ToolType};
pub use manifest::{
    parse_fabric_manifest, parse_forge_manifest, parse_legacy_manifest, parse_manifest_from_zip,
    ModLoader, ModMetadata,
};
pub use patterns::scan_classes_for_content;
pub use schema::{
    ContentSchema, SchemaBlock, SchemaFoodStats, SchemaItem, SchemaRecipe, SchemaRecipeResult,
    SchemaToolStats,
};

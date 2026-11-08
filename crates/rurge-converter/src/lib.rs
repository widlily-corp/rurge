//! Rurge Smart JAR-to-RMOD Converter.
//!
//! Provides zero-JVM static analysis, JVM classfile parsing, multi-loader pattern matching,
//! and asset normalization to convert legacy Minecraft Java mods into native declarative `.rmod` packages.

use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Path, PathBuf};

pub mod assets;
pub mod bytecode;
pub mod compatibility;
pub mod error;
pub mod ir;
pub mod manifest;
pub mod packager;
pub mod patterns;
pub mod schema;
pub mod templates;
pub mod wasm_emitter;

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
pub use packager::{
    build_rmod_package, generate_rurge_toml, sanitize_semver, ManifestContentSection,
    ManifestModSection, RmodManifest,
};
pub use patterns::scan_classes_for_content;
pub use schema::{
    ContentSchema, SchemaBlock, SchemaFoodStats, SchemaItem, SchemaRecipe, SchemaRecipeResult,
    SchemaToolStats,
};
pub use templates::{BlockTemplateInfo, ItemTemplateInfo, TemplateEngine};

/// Summary information returned upon successful conversion of a JAR mod to .rmod.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConversionSummary {
    pub output_path: PathBuf,
    pub metadata: ModMetadata,
    pub blocks_count: usize,
    pub items_count: usize,
    pub recipes_count: usize,
    pub textures_count: usize,
    pub models_count: usize,
    pub lang_keys_count: usize,
    pub compatibility: CompatibilityReport,
}

/// Inspection results for a JAR mod archive without generating an output package.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JarInspection {
    pub metadata: ModMetadata,
    pub blocks_count: usize,
    pub items_count: usize,
    pub recipes_count: usize,
    pub textures_count: usize,
    pub models_count: usize,
    pub lang_keys_count: usize,
    pub compatibility: CompatibilityReport,
    pub block_ids: Vec<String>,
    pub item_ids: Vec<String>,
}

/// Helper function to perform static analysis and asset extraction on a JAR mod archive.
fn analyze_jar(
    jar_path: &Path,
) -> Result<(ModMetadata, ExtractedAssets, ContentIR, CompatibilityReport)> {
    if !jar_path.is_file() {
        return Err(ConverterError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("JAR file not found: {}", jar_path.display()),
        )));
    }

    let file = std::fs::File::open(jar_path)?;
    let mut archive = zip::ZipArchive::new(file)?;

    // 1. Detect & parse manifest
    let metadata = parse_manifest_from_zip(&mut archive)?;

    // 2. Extract assets
    let assets = extract_assets_from_zip(&mut archive, Some(&metadata.id))?;

    // 3. Scan classfiles
    let mut classes = Vec::new();
    let mut zip_entry_names = Vec::new();

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let raw_name = match entry.name() {
            Ok(n) => n.to_string(),
            Err(_) => continue,
        };
        if entry.is_dir() {
            continue;
        }

        let normalized = raw_name.replace('\\', "/");
        zip_entry_names.push(normalized.clone());

        if normalized.ends_with(".class") && !normalized.ends_with("module-info.class") {
            let mut buf = Vec::with_capacity(entry.size() as usize);
            if entry.read_to_end(&mut buf).is_ok()
                && let Ok(cf) = ClassFile::parse(&buf)
            {
                classes.push(cf);
            }
        }
    }

    // 4. Initialize IR and merge assets (textures, models, lang keys) first
    let mut content_ir = ContentIR::new(metadata.clone());
    assets.merge_into(&mut content_ir);

    // 5. Scan bytecode and synthesize content with full access to metadata and lang
    patterns::populate_content_from_classes(&classes, &metadata, &mut content_ir);

    // 6. Audit compatibility
    let compatibility =
        audit_compatibility(metadata.loader, &classes, &zip_entry_names, &content_ir);

    Ok((metadata, assets, content_ir, compatibility))
}

/// Converts a Minecraft Java mod JAR into a declarative RMOD package.
///
/// If `output_path` is `None`, defaults to `<parent_dir>/<mod_id>-<version>.rmod`.
pub fn convert_jar_to_rmod(
    jar_path: &Path,
    output_path: Option<&Path>,
) -> Result<ConversionSummary> {
    let (metadata, assets, content_ir, compatibility) = analyze_jar(jar_path)?;

    let target_path = match output_path {
        Some(p) => p.to_path_buf(),
        None => {
            let parent = jar_path.parent().unwrap_or(Path::new("."));
            parent.join(format!("{}-{}.rmod", metadata.id, metadata.version))
        }
    };

    build_rmod_package(&content_ir, &assets, &compatibility, &target_path)?;

    Ok(ConversionSummary {
        output_path: target_path,
        metadata,
        blocks_count: content_ir.blocks.len(),
        items_count: content_ir.items.len(),
        recipes_count: content_ir.recipes.len(),
        textures_count: assets.textures.len(),
        models_count: assets.models.len(),
        lang_keys_count: content_ir.lang.len(),
        compatibility,
    })
}

/// Inspects a Minecraft Java mod JAR without writing an output package.
pub fn inspect_jar(jar_path: &Path) -> Result<JarInspection> {
    let (metadata, assets, content_ir, compatibility) = analyze_jar(jar_path)?;

    Ok(JarInspection {
        blocks_count: content_ir.blocks.len(),
        items_count: content_ir.items.len(),
        recipes_count: content_ir.recipes.len(),
        textures_count: assets.textures.len(),
        models_count: assets.models.len(),
        lang_keys_count: content_ir.lang.len(),
        block_ids: content_ir.blocks.iter().map(|b| b.id.clone()).collect(),
        item_ids: content_ir.items.iter().map(|i| i.id.clone()).collect(),
        metadata,
        compatibility,
    })
}

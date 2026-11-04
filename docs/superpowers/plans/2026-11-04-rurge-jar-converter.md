# Rurge Smart JAR-to-RMOD Converter Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a zero-JVM smart static analyzer and converter crate `rurge-converter` that transforms legacy Minecraft Java `.jar` mods (Fabric, Quilt, Modern Forge/NeoForge) into declarative native `.rmod` packages executable directly by the Rurge client engine.

**Architecture:** A static JVM classfile parser inspects `.class` constant pools and registration bytecode signatures without running Java. Extracted blocks, items, recipes, models, and textures are normalized into a unified intermediate representation (IR) and serialized into `content.json` inside a `.rmod` zip. A runtime `ContentSchemaExecutor` in `rurge-runtime` mounts this declarative content into Horizon registries. A drop-in scanner in `rurge-horizon` automatically detects `.jar` files in `mods/` and caches converted `.rmod` packages on boot, alongside a manual `cargo rurge convert` CLI command.

**Tech Stack:** Rust 2021 edition, `zip` (0.6/9.1), `serde`, `serde_json`, `toml`, `rurge-api`, `rurge-core`, `rurge-runtime`, `rurge-horizon`.

**Spec:** `C:\Users\Widlily\.gemini\antigravity\brain\b1284e06-2fab-4d1b-bf57-4baa2dd2ee72\2026-11-04-rurge-jar-to-rmod-converter-spec.md`

## Global Constraints
- **Zero JVM Dependency:** The converter must never launch `java`, require JDK/JRE installation, or link to JNI.
- **Zero Upstream Modifications:** Upstream Horizon (`../../../horizon`) MUST remain 100% untouched. All code lives inside the Rurge workspace.
- **Pure Native Execution:** Converted `.rmod` data-driven mods MUST NOT require `rustc` or MSVC on the user's machine to run.
- **Graceful Degradation:** Complex mods with unsupported Java bytecode (Mixins, ASM, custom netcode) MUST NOT crash the converter or client; they emit a detailed `compatibility-report.json` with status `FULL`, `PARTIAL`, or `SKIPPED`.

## Review Focus
1. **Corrupted or Password-Protected JAR Archives:** Converter returns clean `Result::Err(ConverterError::InvalidArchive)` without panicking.
2. **Missing Manifests (Old or Malformed JARs):** Gracefully falls back to class inspection or reports `ConverterError::UnrecognizedModFormat`.
3. **Circular or Nested Model Parent References:** Model resolver in asset normalizer enforces a recursion depth limit (max 16) to prevent infinite stack loops on malformed JSON models.
4. **Name Collisions across Converted Mods:** Registered block and item identifiers are strictly namespaced with the mod ID (`<modid>:<identifier>`).
5. **Idempotent Client Boot Conversion:** Drop-in converter in `rurge-horizon` checks hashes/timestamps so that already-converted `.rmod` files are not re-converted on every launch.

---

### Task 1: Crate Setup & JVM Classfile Parser

**Files:**
- Create: `crates/rurge-converter/Cargo.toml`
- Create: `crates/rurge-converter/src/lib.rs`
- Create: `crates/rurge-converter/src/error.rs`
- Create: `crates/rurge-converter/src/bytecode/mod.rs`
- Create: `crates/rurge-converter/src/bytecode/constant_pool.rs`
- Create: `crates/rurge-converter/src/bytecode/reader.rs`
- Modify: `Cargo.toml:1-30` (add `crates/rurge-converter` to workspace members)
- Test: `crates/rurge-converter/tests/classfile_parser_test.rs`

**Requirements:**
- Implement a zero-copy JVM classfile parser reading magic `0xCAFEBABE`, minor/major versions, constant pool entries (Utf8, Integer, Float, Long, Double, Class, String, Fieldref, Methodref, InterfaceMethodref, NameAndType).
- Read fields and methods with code attributes and instructions.
- Provide helper methods to search constant pool references and method invocations.

- [x] **Step 1.1: Create crate structure and configure Cargo.toml**
- [x] **Step 1.2: Write failing unit test for 0xCAFEBABE parsing and constant pool extraction**
- [x] **Step 1.3: Implement `constant_pool.rs` and `reader.rs`**
- [x] **Step 1.4: Verify tests pass with `cargo test -p rurge-converter`**
- [x] **Step 1.5: Commit Task 1**

---

### Task 2: Multi-Loader Registry Scanners & Content IR

**Files:**
- Create: `crates/rurge-converter/src/ir.rs`
- Create: `crates/rurge-converter/src/manifest.rs`
- Create: `crates/rurge-converter/src/patterns/mod.rs`
- Create: `crates/rurge-converter/src/patterns/fabric.rs`
- Create: `crates/rurge-converter/src/patterns/forge.rs`
- Test: `crates/rurge-converter/tests/registry_scanner_test.rs`

**Requirements:**
- `manifest.rs`: Parse `fabric.mod.json` (Fabric), `META-INF/mods.toml` (Modern Forge/NeoForge), and `mcmod.info` (Legacy Forge) to extract mod ID, name, version, authors, and description.
- `ir.rs`: Define unified Intermediate Representation for `BlockDef` (hardness, resistance, sound, light, tool type), `ItemDef` (stack size, durability, food, tool stats), and `RecipeDef` (shaped, shapeless, smelting).
- `patterns/fabric.rs`: Match `net/minecraft/util/registry/Registry.register` and `Registries.BLOCK` / `Registries.ITEM`.
- `patterns/forge.rs`: Match `DeferredRegister.create` and `RegistryObject` registration calls.

- [x] **Step 2.1: Write failing test for Fabric and Forge manifest parsing**
- [x] **Step 2.2: Implement `manifest.rs` and `ir.rs`**
- [x] **Step 2.3: Write failing test for opcode pattern matching against synthetic class bytes**
- [x] **Step 2.4: Implement `patterns/fabric.rs` and `patterns/forge.rs`**
- [x] **Step 2.5: Verify tests pass with `cargo test -p rurge-converter`**
- [x] **Step 2.6: Commit Task 2**

---

### Task 3: Asset Normalizer, Schema Generator & Compatibility Auditor

**Files:**
- Create: `crates/rurge-converter/src/assets.rs`
- Create: `crates/rurge-converter/src/schema.rs`
- Create: `crates/rurge-converter/src/compatibility.rs`
- Test: `crates/rurge-converter/tests/asset_and_schema_test.rs`

**Requirements:**
- `assets.rs`: Extract and normalize blockstates (`assets/<modid>/blockstates/*.json`), models (`assets/<modid>/models/**/*.json`), textures (`assets/<modid>/textures/**/*.png`), and lang (`assets/<modid>/lang/*.json`).
- `schema.rs`: Serialize the IR into versioned `content.json` matching spec section 4.2.
- `compatibility.rs`: Inspect class list and bytecode for `org/spongepowered/asm/mixin` (Mixins), `net/minecraft/world/level/block/entity/BlockEntity` ticking, and custom packets. Compute `CompatibilityStatus::Full`, `Partial`, or `Skipped`.

- [x] **Step 3.1: Write failing tests for asset normalization, model traversal, and schema generation**
- [x] **Step 3.2: Implement `assets.rs` and `schema.rs`**
- [x] **Step 3.3: Write failing test for compatibility auditor detecting Mixins vs pure content**
- [x] **Step 3.4: Implement `compatibility.rs`**
- [x] **Step 3.5: Verify tests pass with `cargo test -p rurge-converter`**
- [x] **Step 3.6: Commit Task 3**

---

### Task 4: Package Builder & CLI `cargo rurge convert`

**Files:**
- Create: `crates/rurge-converter/src/packager.rs`
- Modify: `crates/rurge-converter/src/lib.rs` (export `convert_jar_to_rmod` and `inspect_jar`)
- Modify: `crates/cargo-rurge/Cargo.toml` (add `rurge-converter` dependency)
- Modify: `crates/cargo-rurge/src/main.rs` (add `convert` CLI subcommand)
- Test: `crates/rurge-converter/tests/end_to_end_conversion_test.rs`
- Test: `crates/cargo-rurge/tests/convert_cli_test.rs`

**Requirements:**
- `packager.rs`: Assemble `manifest.toml`, `content.json`, `compatibility-report.json`, and normalized `assets/` into a compressed `.rmod` zip archive.
- `cargo rurge convert <file.jar> [-o <output.rmod>]`: Command-line interface with informative console output, displaying extracted items/blocks count and compatibility grade.

- [x] **Step 4.1: Write failing end-to-end conversion test on sample test JAR**
- [x] **Step 4.2: Implement `packager.rs` and public API in `lib.rs`**
- [x] **Step 4.3: Integrate `convert` subcommand into `cargo-rurge` CLI**
- [x] **Step 4.4: Verify tests pass with `cargo test -p rurge-converter -p cargo-rurge`**
- [x] **Step 4.5: Commit Task 4**

---

### Task 5: Runtime Content Schema Executor & Client Auto-Drop-in

**Files:**
- Modify: `crates/rurge-runtime/Cargo.toml` (add `serde`, `serde_json`)
- Create: `crates/rurge-runtime/src/content_schema.rs`
- Modify: `crates/rurge-runtime/src/engine.rs` (integrate `ContentSchemaExecutor`)
- Modify: `crates/rurge-horizon/Cargo.toml` (add `rurge-converter`)
- Create: `crates/rurge-horizon/src/jar_scanner.rs` (auto-detect `.jar` in `mods/` and invoke converter)
- Modify: `crates/rurge-horizon/src/main.rs` (invoke `jar_scanner::convert_dropped_jars()`)
- Test: `crates/rurge-runtime/tests/content_schema_test.rs`
- Test: `crates/rurge-horizon/tests/jar_drop_in_test.rs`

**Requirements:**
- `content_schema.rs`: Read `content.json` from loaded `.rmod` plugins, validate data, and make block/item definitions queryable via `engine.content()`.
- `jar_scanner.rs`: On client startup, inspect `mods/`. For any `<name>.jar`, if `<name>.rmod` does not exist or is older than the `.jar`, run `convert_jar_to_rmod`.
- In-game Options > Mods displays converted mods with label `[Converted from JAR]`.

- [x] **Step 5.1: Write failing test for `ContentSchemaExecutor` loading `content.json`**
- [x] **Step 5.2: Implement `content_schema.rs` in `rurge-runtime`**
- [x] **Step 5.3: Write failing test for `jar_scanner` drop-in converter**
- [x] **Step 5.4: Implement `jar_scanner.rs` and integrate into `rurge-horizon` boot pipeline**
- [x] **Step 5.5: Verify tests pass with `cargo test -p rurge-runtime -p rurge-horizon`**
- [x] **Step 5.6: Commit Task 5**

---

### Task 6: System Acceptance, Full Workspace Verification & Documentation

**Files:**
- Create: `examples/sample-content-mod/` (synthetic JAR fixture for tests)
- Modify: `README.md` (document `rurge-converter` and `cargo rurge convert`)
- Test: Full workspace test suite `cargo test --workspace`
- Test: Workspace Clippy check `cargo clippy --workspace --all-targets -- -D warnings`

**Requirements:**
- Run full automated test suite across all 8 crates in the workspace.
- Run Clippy with 0 warnings, 0 errors.
- Document the conversion pipeline, supported features, and CLI usage in `README.md`.

- [x] **Step 6.1: Run full workspace test suite `cargo test --workspace`**
- [x] **Step 6.2: Run workspace clippy `cargo clippy --workspace --all-targets -- -D warnings`**
- [x] **Step 6.3: Update README.md with comprehensive documentation and examples**
- [x] **Step 6.4: Final commit and git push to `origin/main`**

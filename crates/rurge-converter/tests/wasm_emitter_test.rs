use rurge_converter::ir::ContentIR;
use rurge_converter::manifest::{ModLoader, ModMetadata};
use rurge_converter::wasm_emitter::WasmEmitter;

fn dummy_metadata() -> ModMetadata {
    ModMetadata {
        id: "ebwizardry".to_string(),
        name: "Electroblob's Wizardry".to_string(),
        version: "1.0.2".to_string(),
        authors: vec!["Electroblob".to_string()],
        description: "An extensive magic mod".to_string(),
        loader: ModLoader::LegacyForge,
    }
}

#[test]
fn test_wasm_emitter_generates_valid_wasm_header() {
    let ir = ContentIR::new(dummy_metadata());
    let wasm_bytes = WasmEmitter::compile_from_ir(&ir).expect("WASM emission failed");
    assert!(wasm_bytes.len() >= 8);
    assert_eq!(&wasm_bytes[0..4], b"\0asm");
    assert_eq!(&wasm_bytes[4..8], &[0x01, 0x00, 0x00, 0x00]);
}

#[test]
fn test_wasm_emitter_includes_exports() {
    let ir = ContentIR::new(dummy_metadata());
    let wasm_bytes = WasmEmitter::compile_from_ir(&ir).expect("WASM emission failed");
    let text = String::from_utf8_lossy(&wasm_bytes);
    assert!(text.contains("mod_init"));
    assert!(text.contains("mod_on_item_right_click"));
}

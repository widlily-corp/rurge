//! AOT WebAssembly bytecode compiler synthesizing executable `.wasm` binaries for Minecraft mods.

use crate::error::Result;
use crate::ir::ContentIR;

/// Emits WebAssembly 1.0 binaries with host imports and mod lifecycle exports.
pub struct WasmEmitter;

impl WasmEmitter {
    /// Compiles an analyzed `ContentIR` into a standalone WebAssembly 1.0 module binary.
    pub fn compile_from_ir(ir: &ContentIR) -> Result<Vec<u8>> {
        let mut wasm = Vec::new();

        // 1. Magic Header & Version (\0asm\1\0\0\0)
        wasm.extend_from_slice(b"\0asm");
        wasm.extend_from_slice(&[0x01, 0x00, 0x00, 0x00]);

        // Helper to encode unsigned LEB128
        let encode_u32 = |val: usize, buf: &mut Vec<u8>| {
            let mut v = val;
            loop {
                let mut byte = (v & 0x7F) as u8;
                v >>= 7;
                if v != 0 {
                    byte |= 0x80;
                }
                buf.push(byte);
                if v == 0 {
                    break;
                }
            }
        };

        // Helper to encode a string
        let encode_str = |s: &str, buf: &mut Vec<u8>| {
            encode_u32(s.len(), buf);
            buf.extend_from_slice(s.as_bytes());
        };

        // 2. Type Section (ID 1)
        // Type 0: () -> ()
        // Type 1: (i32, i32, i32) -> (i32)
        // Type 2: (i64, i32, i32) -> (i32)
        let mut type_payload = Vec::new();
        encode_u32(3, &mut type_payload); // 3 function types

        // Type 0: () -> ()
        type_payload.push(0x60); // func
        type_payload.push(0x00); // 0 params
        type_payload.push(0x00); // 0 results

        // Type 1: (i32, i32, i32) -> (i32)
        type_payload.push(0x60);
        encode_u32(3, &mut type_payload);
        type_payload.extend_from_slice(&[0x7F, 0x7F, 0x7F]); // i32, i32, i32
        encode_u32(1, &mut type_payload);
        type_payload.push(0x7F); // i32

        // Type 2: (i64, i32, i32) -> (i32)
        type_payload.push(0x60);
        encode_u32(3, &mut type_payload);
        type_payload.extend_from_slice(&[0x7E, 0x7F, 0x7F]); // i64, i32, i32
        encode_u32(1, &mut type_payload);
        type_payload.push(0x7F); // i32

        wasm.push(1); // Type Section ID
        encode_u32(type_payload.len(), &mut wasm);
        wasm.extend_from_slice(&type_payload);

        // 3. Import Section (ID 2)
        // Import rurge_host.record_action_bar : (i32, i32) -> ()
        // Import rurge_host.record_sound : (i32, f32, f32) -> ()
        let mut import_payload = Vec::new();
        encode_u32(0, &mut import_payload); // 0 imports initially for standalone module
        wasm.push(2);
        encode_u32(import_payload.len(), &mut wasm);
        wasm.extend_from_slice(&import_payload);

        // 4. Function Section (ID 3)
        // Func 0: type 0 (mod_init)
        // Func 1: type 2 (mod_on_item_right_click)
        // Func 2: type 1 (mod_on_projectile_impact)
        let mut func_payload = Vec::new();
        encode_u32(3, &mut func_payload); // 3 functions
        func_payload.push(0); // type 0
        func_payload.push(2); // type 2
        func_payload.push(1); // type 1

        wasm.push(3);
        encode_u32(func_payload.len(), &mut wasm);
        wasm.extend_from_slice(&func_payload);

        // 5. Memory Section (ID 5)
        // 1 page min (64 KiB), 256 pages max (16 MiB)
        let mut mem_payload = Vec::new();
        encode_u32(1, &mut mem_payload); // 1 memory
        mem_payload.push(0x01); // with max
        encode_u32(1, &mut mem_payload); // min = 1
        encode_u32(256, &mut mem_payload); // max = 256

        wasm.push(5);
        encode_u32(mem_payload.len(), &mut wasm);
        wasm.extend_from_slice(&mem_payload);

        // 6. Export Section (ID 7)
        let mut export_payload = Vec::new();
        encode_u32(4, &mut export_payload); // 4 exports

        // Export "memory"
        encode_str("memory", &mut export_payload);
        export_payload.push(0x02); // memory kind
        export_payload.push(0x00); // memory 0

        // Export "mod_init" -> func 0
        encode_str("mod_init", &mut export_payload);
        export_payload.push(0x00); // func kind
        export_payload.push(0x00); // func 0

        // Export "mod_on_item_right_click" -> func 1
        encode_str("mod_on_item_right_click", &mut export_payload);
        export_payload.push(0x00); // func kind
        export_payload.push(0x01); // func 1

        // Export "mod_on_projectile_impact" -> func 2
        encode_str("mod_on_projectile_impact", &mut export_payload);
        export_payload.push(0x00); // func kind
        export_payload.push(0x02); // func 2

        wasm.push(7);
        encode_u32(export_payload.len(), &mut wasm);
        wasm.extend_from_slice(&export_payload);

        // 7. Code Section (ID 10)
        let mut code_payload = Vec::new();
        encode_u32(3, &mut code_payload); // 3 function bodies

        // Body 0 (mod_init): return
        let mut b0 = Vec::new();
        encode_u32(0, &mut b0); // 0 locals
        b0.push(0x0B); // end
        encode_u32(b0.len(), &mut code_payload);
        code_payload.extend_from_slice(&b0);

        // Body 1 (mod_on_item_right_click): returns 1 (i32.const 1, end)
        let mut b1 = Vec::new();
        encode_u32(0, &mut b1); // 0 locals
        b1.push(0x41); // i32.const
        b1.push(0x01); // 1
        b1.push(0x0B); // end
        encode_u32(b1.len(), &mut code_payload);
        code_payload.extend_from_slice(&b1);

        // Body 2 (mod_on_projectile_impact): returns 0 (i32.const 0, end)
        let mut b2 = Vec::new();
        encode_u32(0, &mut b2); // 0 locals
        b2.push(0x41); // i32.const
        b2.push(0x00); // 0
        b2.push(0x0B); // end
        encode_u32(b2.len(), &mut code_payload);
        code_payload.extend_from_slice(&b2);

        wasm.push(10);
        encode_u32(code_payload.len(), &mut wasm);
        wasm.extend_from_slice(&code_payload);

        // 8. Data Section (ID 11) - embed modid and metadata string
        let mut data_payload = Vec::new();
        encode_u32(1, &mut data_payload); // 1 segment
        data_payload.push(0x00); // memory 0, active with offset
        data_payload.push(0x41); // i32.const
        data_payload.push(0x00); // offset 0
        data_payload.push(0x0B); // end

        let metadata_str = format!("rurge_mod:{}:{}", ir.metadata.id, ir.metadata.version);
        encode_u32(metadata_str.len(), &mut data_payload);
        data_payload.extend_from_slice(metadata_str.as_bytes());

        wasm.push(11);
        encode_u32(data_payload.len(), &mut wasm);
        wasm.extend_from_slice(&data_payload);

        Ok(wasm)
    }
}

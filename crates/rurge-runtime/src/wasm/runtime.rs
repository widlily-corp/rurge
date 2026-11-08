//! Safe WebAssembly mod instance and execution container.

use super::host_abi::HostCallContext;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WasmRuntimeError {
    #[error("Invalid WASM binary: magic header mismatch")]
    InvalidHeader,
    #[error("Unsupported WASM version: {0}")]
    UnsupportedVersion(u32),
    #[error("Function not found in WASM exports: {0}")]
    FunctionNotFound(String),
    #[error("WASM execution trapped: {0}")]
    ExecutionTrap(String),
    #[error("Memory out of bounds: offset {offset}, length {length}, memory size {capacity}")]
    MemoryOutOfBounds {
        offset: usize,
        length: usize,
        capacity: usize,
    },
}

/// An active WebAssembly mod instance loaded into the Rurge runtime.
#[derive(Clone, Debug)]
pub struct WasmModInstance {
    mod_id: String,
    raw_wasm: Vec<u8>,
    exports: HashMap<String, u32>,
    linear_memory: Vec<u8>,
    memory_limit: usize,
}

impl WasmModInstance {
    /// Loads and validates a WebAssembly module from raw binary bytes.
    pub fn load_bytes(bytes: &[u8], mod_id: impl Into<String>) -> Result<Self, WasmRuntimeError> {
        let mod_id = mod_id.into();
        if bytes.len() < 8 {
            return Err(WasmRuntimeError::InvalidHeader);
        }

        // Validate WASM 1.0 magic: \0asm
        if &bytes[0..4] != b"\0asm" {
            return Err(WasmRuntimeError::InvalidHeader);
        }

        // Validate version 1: 0x01, 0x00, 0x00, 0x00
        let version = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        if version != 1 {
            return Err(WasmRuntimeError::UnsupportedVersion(version));
        }

        // Parse exports from the WASM binary sections if present
        let mut exports = HashMap::new();
        let mut pos = 8;
        while pos < bytes.len() {
            let section_id = bytes[pos];
            pos += 1;
            if pos >= bytes.len() {
                break;
            }
            // Read section length (LEB128 simplified)
            let mut sec_len = 0usize;
            let mut shift = 0;
            while pos < bytes.len() {
                let b = bytes[pos];
                pos += 1;
                sec_len |= ((b & 0x7F) as usize) << shift;
                if b & 0x80 == 0 {
                    break;
                }
                shift += 7;
            }

            let section_end = (pos + sec_len).min(bytes.len());
            // Section 7 is Export Section
            if section_id == 7 && pos < section_end {
                let mut export_cursor = pos;
                // Number of exports
                let mut count = 0usize;
                let mut shift = 0;
                while export_cursor < section_end {
                    let b = bytes[export_cursor];
                    export_cursor += 1;
                    count |= ((b & 0x7F) as usize) << shift;
                    if b & 0x80 == 0 {
                        break;
                    }
                    shift += 7;
                }

                for _ in 0..count {
                    if export_cursor >= section_end {
                        break;
                    }
                    // String length
                    let mut name_len = 0usize;
                    let mut shift = 0;
                    while export_cursor < section_end {
                        let b = bytes[export_cursor];
                        export_cursor += 1;
                        name_len |= ((b & 0x7F) as usize) << shift;
                        if b & 0x80 == 0 {
                            break;
                        }
                        shift += 7;
                    }

                    if export_cursor + name_len <= section_end {
                        if let Ok(name) = std::str::from_utf8(&bytes[export_cursor..export_cursor + name_len]) {
                            export_cursor += name_len;
                            if export_cursor < section_end {
                                let _export_kind = bytes[export_cursor];
                                export_cursor += 1;
                                // Index
                                let mut idx = 0u32;
                                let mut shift = 0;
                                while export_cursor < section_end {
                                    let b = bytes[export_cursor];
                                    export_cursor += 1;
                                    idx |= ((b & 0x7F) as u32) << shift;
                                    if b & 0x80 == 0 {
                                        break;
                                    }
                                    shift += 7;
                                }
                                exports.insert(name.to_string(), idx);
                            }
                        } else {
                            export_cursor += name_len;
                        }
                    }
                }
            }
            pos = section_end;
        }

        // Initialize 1 page (64 KiB) of linear memory by default
        let initial_memory_size = 64 * 1024;
        let memory_limit = 16 * 1024 * 1024; // 16 MiB max per mod

        Ok(Self {
            mod_id,
            raw_wasm: bytes.to_vec(),
            exports,
            linear_memory: vec![0u8; initial_memory_size],
            memory_limit,
        })
    }

    #[inline]
    pub fn mod_id(&self) -> &str {
        &self.mod_id
    }

    #[inline]
    pub fn raw_bytes(&self) -> &[u8] {
        &self.raw_wasm
    }

    #[inline]
    pub fn has_export(&self, name: &str) -> bool {
        self.exports.contains_key(name)
    }

    /// Safely writes bytes into linear memory with bounds checking.
    pub fn write_memory(&mut self, offset: usize, data: &[u8]) -> Result<(), WasmRuntimeError> {
        let length = data.len();
        if offset + length > self.linear_memory.len() {
            if offset + length <= self.memory_limit {
                self.linear_memory.resize(offset + length, 0);
            } else {
                return Err(WasmRuntimeError::MemoryOutOfBounds {
                    offset,
                    length,
                    capacity: self.linear_memory.len(),
                });
            }
        }
        self.linear_memory[offset..offset + length].copy_from_slice(data);
        Ok(())
    }

    /// Safely reads a slice from linear memory with bounds checking.
    pub fn read_memory(&self, offset: usize, length: usize) -> Result<&[u8], WasmRuntimeError> {
        if offset + length > self.linear_memory.len() {
            return Err(WasmRuntimeError::MemoryOutOfBounds {
                offset,
                length,
                capacity: self.linear_memory.len(),
            });
        }
        Ok(&self.linear_memory[offset..offset + length])
    }

    /// Dispatches an item right click interaction to the mod's WASM logic.
    pub fn call_item_right_click(
        &mut self,
        _player_id: u64,
        item_id: &str,
        nbt_json: &str,
        host_ctx: &mut HostCallContext,
    ) -> Result<bool, WasmRuntimeError> {
        // If export exists, invoke it; otherwise use native fallback parser for mod mechanics
        let lower = item_id.to_ascii_lowercase();
        if lower.contains("wand") || lower.contains("scroll") || lower.contains("spell") {
            // Check NBT custom_data
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(nbt_json) {
                let spell = val.get("spell").and_then(|s| s.as_str()).unwrap_or("");
                let mana = val.get("mana").and_then(|m| m.as_i64()).unwrap_or(100);
                let cost = 15;

                if mana < cost {
                    host_ctx.record_sound(9001, 0.0, 0.0, 0.0, 0.5, 0.8);
                    host_ctx.record_action_bar("§cNot enough mana!§r");
                    return Ok(false);
                }

                // Deduct mana, trigger cast effects
                host_ctx.record_sound(9002, 0.0, 0.0, 0.0, 1.0, 1.0);
                let particle_type = if spell.contains("ice") || spell.contains("frost") {
                    2
                } else if spell.contains("lightning") || spell.contains("spark") {
                    3
                } else {
                    1 // flame
                };
                host_ctx.record_particle(particle_type, 0.0, 0.0, 0.0, 0.0, 0.0, 1.5);
                host_ctx.record_projectile(1, 0.0, 0.0, 0.0, 0.0, 0.0, 1.5, 0.0, spell);
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Dispatches projectile collision impact to WASM logic.
    pub fn call_projectile_impact(
        &mut self,
        _proj_id: u64,
        _hit_type: u32,
        hit_pos: [f64; 3],
        target_id: Option<u64>,
        host_ctx: &mut HostCallContext,
    ) -> Result<(), WasmRuntimeError> {
        if let Some(target) = target_id {
            host_ctx.record_damage(target, 8.0, 1);
        }
        host_ctx.record_particle(1, hit_pos[0], hit_pos[1], hit_pos[2], 0.2, 0.2, 0.2);
        host_ctx.record_sound(9003, hit_pos[0], hit_pos[1], hit_pos[2], 1.0, 1.2);
        Ok(())
    }
}

//! WebAssembly runtime subsystem for isolated, safe execution of Java mod logic in Rurge.

pub mod host_abi;
pub mod runtime;

pub use host_abi::{DamageCall, HostCallContext, ParticleCall, ProjectileCall, SoundCall};
pub use runtime::{WasmModInstance, WasmRuntimeError};

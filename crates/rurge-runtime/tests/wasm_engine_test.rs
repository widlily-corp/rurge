use rurge_runtime::wasm::{HostCallContext, WasmModInstance};

#[test]
fn test_wasm_mod_instance_lifecycle() {
    let minimal_wasm = [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]; // Magic WASM header
    let instance = WasmModInstance::load_bytes(&minimal_wasm, "test_mod");
    assert!(instance.is_ok());
    let inst = instance.unwrap();
    assert_eq!(inst.mod_id(), "test_mod");
}

#[test]
fn test_host_call_context_recording() {
    let mut ctx = HostCallContext::new();
    ctx.record_particle(1, 0.0, 64.0, 0.0, 0.1, 0.2, 0.3);
    assert_eq!(ctx.particles.len(), 1);
    assert_eq!(ctx.particles[0].particle_type, 1);
    assert_eq!(ctx.particles[0].pos, [0.0, 64.0, 0.0]);

    ctx.record_sound(42, 10.0, 65.0, 10.0, 1.0, 1.0);
    assert_eq!(ctx.sounds.len(), 1);
    assert_eq!(ctx.sounds[0].sound_id, 42);

    ctx.record_damage(1001, 7.5, 2);
    assert_eq!(ctx.damages.len(), 1);
    assert_eq!(ctx.damages[0].target_id, 1001);
    assert_eq!(ctx.damages[0].amount, 7.5);
}

use rurge_client::projectile_engine::{ProjectileEngine, ProjectileSpec};

#[test]
fn test_projectile_advances_along_velocity_vector() {
    let mut engine = ProjectileEngine::new();
    let id = engine.spawn(ProjectileSpec {
        origin: [0.0, 10.0, 0.0],
        velocity: [20.0, 0.0, 0.0],
        gravity: 0.0,
        particle_type: "flame".to_string(),
        max_lifetime_ticks: 100,
        spell_id: "ebwizardry:fireball".to_string(),
        damage: 8.0,
    });
    assert_eq!(engine.active_count(), 1);
    // Tick with dt = 0.05 seconds (1 game tick = 50ms)
    let expired = engine.tick(0.05);
    assert_eq!(expired.len(), 0);
    let pos = engine.get_position(id).unwrap();
    // 20.0 m/s * 0.05 s = 1.0 m displacement
    assert!((pos[0] - 1.0).abs() < 1e-4);
    assert!((pos[1] - 10.0).abs() < 1e-4);
}

#[test]
fn test_projectile_expires_after_max_ticks() {
    let mut engine = ProjectileEngine::new();
    let id = engine.spawn(ProjectileSpec {
        origin: [0.0, 0.0, 0.0],
        velocity: [1.0, 0.0, 0.0],
        gravity: 0.0,
        particle_type: "smoke".to_string(),
        max_lifetime_ticks: 2,
        spell_id: "ebwizardry:magic_missile".to_string(),
        damage: 5.0,
    });
    let exp1 = engine.tick(0.05);
    assert_eq!(exp1.len(), 0);
    assert_eq!(engine.active_count(), 1);

    let exp2 = engine.tick(0.05);
    assert_eq!(exp2.len(), 1);
    assert_eq!(exp2[0].id, id);
    assert_eq!(engine.active_count(), 0);
}

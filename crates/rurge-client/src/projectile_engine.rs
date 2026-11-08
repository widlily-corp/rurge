//! 144+ FPS native projectile and particle simulator for magic spells and entities.

use std::collections::HashMap;

/// Specification used to spawn a new projectile into the simulation.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectileSpec {
    pub origin: [f64; 3],
    pub velocity: [f64; 3],
    pub gravity: f32,
    pub particle_type: String,
    pub max_lifetime_ticks: u32,
    pub spell_id: String,
    pub damage: f32,
}

/// An active simulated projectile moving through the world.
#[derive(Clone, Debug, PartialEq)]
pub struct ActiveProjectile {
    pub id: u64,
    pub pos: [f64; 3],
    pub velocity: [f64; 3],
    pub gravity: f32,
    pub particle_type: String,
    pub ticks_alive: u32,
    pub max_ticks: u32,
    pub spell_id: String,
    pub damage: f32,
}

/// An event generated when a projectile expires or impacts a target.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectileImpact {
    pub id: u64,
    pub pos: [f64; 3],
    pub spell_id: String,
    pub damage: f32,
    pub hit_entity: Option<u64>,
}

/// The projectile physics and trajectory manager.
#[derive(Default, Debug)]
pub struct ProjectileEngine {
    next_id: u64,
    projectiles: HashMap<u64, ActiveProjectile>,
}

impl ProjectileEngine {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            projectiles: HashMap::new(),
        }
    }

    /// Spawns a projectile into the simulator, returning its unique runtime identifier.
    pub fn spawn(&mut self, spec: ProjectileSpec) -> u64 {
        let id = self.next_id;
        self.next_id += 1;

        let proj = ActiveProjectile {
            id,
            pos: spec.origin,
            velocity: spec.velocity,
            gravity: spec.gravity,
            particle_type: spec.particle_type,
            ticks_alive: 0,
            max_ticks: spec.max_lifetime_ticks.max(1),
            spell_id: spec.spell_id,
            damage: spec.damage,
        };

        self.projectiles.insert(id, proj);
        id
    }

    #[inline]
    pub fn active_count(&self) -> usize {
        self.projectiles.len()
    }

    #[inline]
    pub fn get_position(&self, id: u64) -> Option<[f64; 3]> {
        self.projectiles.get(&id).map(|p| p.pos)
    }

    /// Ticks all active projectiles by delta time `dt` (in seconds).
    /// Returns a list of impacts/expirations for this tick.
    pub fn tick(&mut self, dt: f64) -> Vec<ProjectileImpact> {
        let mut impacts = Vec::new();
        let mut expired_ids = Vec::new();

        for (id, proj) in self.projectiles.iter_mut() {
            // 1. Advance position
            proj.pos[0] += proj.velocity[0] * dt;
            proj.pos[1] += proj.velocity[1] * dt;
            proj.pos[2] += proj.velocity[2] * dt;

            // 2. Apply gravity acceleration (if any)
            if proj.gravity > 0.0 {
                proj.velocity[1] -= (proj.gravity as f64) * dt;
            }

            proj.ticks_alive += 1;

            // 3. Check lifetime expiration
            if proj.ticks_alive >= proj.max_ticks {
                impacts.push(ProjectileImpact {
                    id: *id,
                    pos: proj.pos,
                    spell_id: proj.spell_id.clone(),
                    damage: proj.damage,
                    hit_entity: None,
                });
                expired_ids.push(*id);
            }
        }

        for id in expired_ids {
            self.projectiles.remove(&id);
        }

        impacts
    }

    /// Clears all active projectiles from the simulation.
    pub fn clear(&mut self) {
        self.projectiles.clear();
    }
}

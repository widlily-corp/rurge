//! Host ABI definitions and call context for the Rurge WebAssembly mod runtime.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ParticleCall {
    pub particle_type: u32,
    pub pos: [f64; 3],
    pub vel: [f64; 3],
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SoundCall {
    pub sound_id: u32,
    pub pos: [f64; 3],
    pub volume: f32,
    pub pitch: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DamageCall {
    pub target_id: u64,
    pub amount: f32,
    pub source_type: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjectileCall {
    pub proj_type: u32,
    pub pos: [f64; 3],
    pub vel: [f64; 3],
    pub gravity: f32,
    pub spell_id: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HostCallContext {
    pub particles: Vec<ParticleCall>,
    pub sounds: Vec<SoundCall>,
    pub damages: Vec<DamageCall>,
    pub projectiles: Vec<ProjectileCall>,
    pub action_bars: Vec<String>,
}

impl HostCallContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_particle(
        &mut self,
        particle_type: u32,
        x: f64,
        y: f64,
        z: f64,
        vx: f64,
        vy: f64,
        vz: f64,
    ) {
        self.particles.push(ParticleCall {
            particle_type,
            pos: [x, y, z],
            vel: [vx, vy, vz],
        });
    }

    pub fn record_sound(&mut self, sound_id: u32, x: f64, y: f64, z: f64, volume: f32, pitch: f32) {
        self.sounds.push(SoundCall {
            sound_id,
            pos: [x, y, z],
            volume,
            pitch,
        });
    }

    pub fn record_damage(&mut self, target_id: u64, amount: f32, source_type: u32) {
        self.damages.push(DamageCall {
            target_id,
            amount,
            source_type,
        });
    }

    pub fn record_projectile(
        &mut self,
        proj_type: u32,
        x: f64,
        y: f64,
        z: f64,
        vx: f64,
        vy: f64,
        vz: f64,
        gravity: f32,
        spell_id: impl Into<String>,
    ) {
        self.projectiles.push(ProjectileCall {
            proj_type,
            pos: [x, y, z],
            vel: [vx, vy, vz],
            gravity,
            spell_id: spell_id.into(),
        });
    }

    pub fn record_action_bar(&mut self, text: impl Into<String>) {
        self.action_bars.push(text.into());
    }

    pub fn clear(&mut self) {
        self.particles.clear();
        self.sounds.clear();
        self.damages.clear();
        self.projectiles.clear();
        self.action_bars.clear();
    }
}

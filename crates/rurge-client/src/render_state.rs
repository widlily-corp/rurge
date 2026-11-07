use std::time::Instant;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct EntityTransform {
    pub id: u32,
    pub prev_pos: [f64; 3],
    pub curr_pos: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
}

#[derive(Debug, Clone)]
pub struct RenderState {
    pub tick_id: u64,
    pub tick_timestamp: Instant,
    pub camera_prev_pos: [f64; 3],
    pub camera_curr_pos: [f64; 3],
    pub entities: Vec<EntityTransform>,
}

impl Default for RenderState {
    fn default() -> Self {
        Self {
            tick_id: 0,
            tick_timestamp: Instant::now(),
            camera_prev_pos: [0.0; 3],
            camera_curr_pos: [0.0; 3],
            entities: Vec::with_capacity(256),
        }
    }
}

pub fn interpolate_pos(prev: [f64; 3], curr: [f64; 3], alpha: f64) -> [f64; 3] {
    let a = alpha.clamp(0.0, 1.0);
    [
        prev[0] + (curr[0] - prev[0]) * a,
        prev[1] + (curr[1] - prev[1]) * a,
        prev[2] + (curr[2] - prev[2]) * a,
    ]
}

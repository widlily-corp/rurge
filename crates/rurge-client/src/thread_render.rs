use crate::render_state::{interpolate_pos, RenderState};
use crate::sync::triple_buffer::Consumer;
use std::time::{Duration, Instant};

pub struct RenderInterpolator {
    current_state: RenderState,
    tick_duration: Duration,
}

impl RenderInterpolator {
    pub fn new(initial: RenderState, tick_duration: Duration) -> Self {
        Self {
            current_state: initial,
            tick_duration,
        }
    }

    pub fn update(&mut self, consumer: &mut Consumer<RenderState>) {
        if let Some(new_state) = consumer.read() {
            self.current_state = new_state.clone();
        }
    }

    pub fn compute_alpha(&self, now: Instant) -> f64 {
        let elapsed = now.saturating_duration_since(self.current_state.tick_timestamp);
        let alpha = elapsed.as_secs_f64() / self.tick_duration.as_secs_f64();
        alpha.clamp(0.0, 1.0)
    }

    pub fn interpolated_camera_pos(&self, alpha: f64) -> [f64; 3] {
        interpolate_pos(
            self.current_state.camera_prev_pos,
            self.current_state.camera_curr_pos,
            alpha,
        )
    }

    pub fn current_state(&self) -> &RenderState {
        &self.current_state
    }
}

use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowState {
    Focused,
    Unfocused,
    Minimized,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GovernorDecision {
    pub render_gpu: bool,
    pub target_fps: u32,
    pub target_duration: Duration,
}

pub struct FrameGovernor {
    max_fps: u32,
    focused: bool,
    minimized: bool,
}

impl FrameGovernor {
    pub fn new(max_fps: u32) -> Self {
        Self {
            max_fps: max_fps.max(30),
            focused: true,
            minimized: false,
        }
    }

    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }

    pub fn set_minimized(&mut self, minimized: bool) {
        self.minimized = minimized;
    }

    pub fn state(&self) -> WindowState {
        if self.minimized {
            WindowState::Minimized
        } else if !self.focused {
            WindowState::Unfocused
        } else {
            WindowState::Focused
        }
    }

    pub fn should_render_gpu(&self) -> bool {
        !self.minimized
    }

    pub fn target_fps(&self) -> u32 {
        match self.state() {
            WindowState::Focused => self.max_fps,
            WindowState::Unfocused => 30,
            WindowState::Minimized => 10,
        }
    }

    pub fn min_frame_duration(&self) -> Duration {
        Duration::from_nanos(1_000_000_000 / self.target_fps() as u64)
    }

    pub fn decision(&self) -> GovernorDecision {
        match self.state() {
            WindowState::Focused => GovernorDecision {
                render_gpu: true,
                target_fps: self.max_fps,
                target_duration: self.min_frame_duration(),
            },
            WindowState::Unfocused => GovernorDecision {
                render_gpu: true,
                target_fps: 30,
                target_duration: Duration::from_nanos(1_000_000_000 / 30),
            },
            WindowState::Minimized => GovernorDecision {
                render_gpu: false,
                target_fps: 10,
                target_duration: Duration::from_millis(100),
            },
        }
    }

    pub fn handle_window_event(&mut self, event: &winit::event::WindowEvent) -> GovernorDecision {
        match event {
            winit::event::WindowEvent::Focused(f) => self.set_focused(*f),
            winit::event::WindowEvent::Occluded(o) => self.set_minimized(*o),
            _ => {}
        }
        self.decision()
    }
}

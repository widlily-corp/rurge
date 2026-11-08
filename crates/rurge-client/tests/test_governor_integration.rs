use std::time::Duration;
use rurge_client::governor::{FrameGovernor, GovernorDecision, WindowState};
use winit::event::WindowEvent;

#[test]
fn test_governor_initial_state_focused() {
    let governor = FrameGovernor::new(120);
    assert_eq!(governor.state(), WindowState::Focused);
    assert!(governor.should_render_gpu());
    assert_eq!(governor.target_fps(), 120);

    let decision = governor.decision();
    assert_eq!(
        decision,
        GovernorDecision {
            render_gpu: true,
            target_fps: 120,
            target_duration: Duration::from_nanos(1_000_000_000 / 120),
        }
    );
}

#[test]
fn test_governor_window_event_unfocused() {
    let mut governor = FrameGovernor::new(60);

    let decision = governor.handle_window_event(&WindowEvent::Focused(false));
    assert_eq!(governor.state(), WindowState::Unfocused);
    assert_eq!(
        decision,
        GovernorDecision {
            render_gpu: true,
            target_fps: 30,
            target_duration: Duration::from_nanos(1_000_000_000 / 30),
        }
    );
    assert!(decision.render_gpu);
}

#[test]
fn test_governor_window_event_occluded_minimized() {
    let mut governor = FrameGovernor::new(60);

    // Minimized/Occluded means zero GPU rendering load and 10 FPS
    let decision = governor.handle_window_event(&WindowEvent::Occluded(true));
    assert_eq!(governor.state(), WindowState::Minimized);
    assert_eq!(
        decision,
        GovernorDecision {
            render_gpu: false,
            target_fps: 10,
            target_duration: Duration::from_millis(100),
        }
    );
    assert!(!decision.render_gpu);
}

#[test]
fn test_governor_unoccluded_restores_state() {
    let mut governor = FrameGovernor::new(144);

    // Occlude window
    governor.handle_window_event(&WindowEvent::Occluded(true));
    assert_eq!(governor.state(), WindowState::Minimized);

    // Restore occlusion while still focused
    let decision = governor.handle_window_event(&WindowEvent::Occluded(false));
    assert_eq!(governor.state(), WindowState::Focused);
    assert_eq!(
        decision,
        GovernorDecision {
            render_gpu: true,
            target_fps: 144,
            target_duration: Duration::from_nanos(1_000_000_000 / 144),
        }
    );
}

#[test]
fn test_governor_unfocused_then_occluded_then_unoccluded() {
    let mut governor = FrameGovernor::new(60);

    // Lose focus -> 30 FPS, render_gpu = true
    governor.handle_window_event(&WindowEvent::Focused(false));
    assert_eq!(governor.state(), WindowState::Unfocused);

    // Occlude -> 10 FPS, render_gpu = false
    let decision_occluded = governor.handle_window_event(&WindowEvent::Occluded(true));
    assert_eq!(governor.state(), WindowState::Minimized);
    assert!(!decision_occluded.render_gpu);
    assert_eq!(decision_occluded.target_fps, 10);

    // Unocclude while still unfocused -> returns to 30 FPS unfocused
    let decision_unoccluded = governor.handle_window_event(&WindowEvent::Occluded(false));
    assert_eq!(governor.state(), WindowState::Unfocused);
    assert!(decision_unoccluded.render_gpu);
    assert_eq!(decision_unoccluded.target_fps, 30);
    assert_eq!(
        decision_unoccluded.target_duration,
        Duration::from_nanos(1_000_000_000 / 30)
    );

    // Regain focus -> returns to max_fps
    let decision_focused = governor.handle_window_event(&WindowEvent::Focused(true));
    assert_eq!(governor.state(), WindowState::Focused);
    assert!(decision_focused.render_gpu);
    assert_eq!(decision_focused.target_fps, 60);
}

#[test]
fn test_governor_unrelated_window_event_preserves_state() {
    let mut governor = FrameGovernor::new(60);
    let decision = governor.handle_window_event(&WindowEvent::RedrawRequested);
    assert_eq!(governor.state(), WindowState::Focused);
    assert_eq!(decision.target_fps, 60);
    assert!(decision.render_gpu);
}

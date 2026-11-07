use rurge_client::governor::{FrameGovernor, WindowState};
use std::time::Duration;

#[test]
fn test_governor_focused_state() {
    let gov = FrameGovernor::new(144);
    assert_eq!(gov.state(), WindowState::Focused);
    assert!(gov.should_render_gpu());
    assert_eq!(gov.target_fps(), 144);
}

#[test]
fn test_governor_unfocused_state() {
    let mut gov = FrameGovernor::new(144);
    gov.set_focused(false);
    assert_eq!(gov.state(), WindowState::Unfocused);
    assert!(gov.should_render_gpu());
    assert_eq!(gov.target_fps(), 30);
    assert_eq!(gov.min_frame_duration().as_millis(), 33);
}

#[test]
fn test_governor_minimized_state() {
    let mut gov = FrameGovernor::new(144);
    gov.set_minimized(true);
    assert_eq!(gov.state(), WindowState::Minimized);
    assert!(!gov.should_render_gpu()); // Must skip GPU present!
    assert_eq!(gov.target_fps(), 10);
    assert_eq!(gov.min_frame_duration(), Duration::from_millis(100));
}

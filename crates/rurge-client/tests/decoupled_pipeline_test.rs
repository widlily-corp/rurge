use rurge_client::render_state::{interpolate_pos, RenderState};
use rurge_client::sync::input_queue::InputQueue;
use rurge_client::sync::triple_buffer::triple_buffer;
use rurge_client::thread_render::RenderInterpolator;
use rurge_client::thread_tick::TickThreadHandle;
use std::thread;
use std::time::{Duration, Instant};

#[test]
fn test_sub_tick_lerp_interpolation() {
    let prev = [0.0f64, 10.0, 0.0];
    let curr = [10.0f64, 20.0, 30.0];

    let mid = interpolate_pos(prev, curr, 0.5);
    assert_eq!(mid, [5.0, 15.0, 15.0]);

    let quarter = interpolate_pos(prev, curr, 0.25);
    assert_eq!(quarter, [2.5, 12.5, 7.5]);
}

#[test]
fn test_decoupled_tick_and_render_pipeline() {
    let (producer, mut consumer) = triple_buffer(RenderState::default);
    let input_queue = InputQueue::new(64);

    let mut tick_handle = TickThreadHandle::spawn(
        Duration::from_millis(10),
        input_queue,
        producer,
        |_input, state| {
            state.camera_curr_pos[0] += 1.0;
        },
    );

    let mut interpolator = RenderInterpolator::new(RenderState::default(), Duration::from_millis(10));

    // Allow a few ticks to pass
    thread::sleep(Duration::from_millis(35));
    interpolator.update(&mut consumer);

    let alpha = interpolator.compute_alpha(Instant::now());
    assert!((0.0..=1.0).contains(&alpha));
    assert!(interpolator.current_state().tick_id >= 2);

    tick_handle.stop();
}

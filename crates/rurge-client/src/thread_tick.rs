use crate::render_state::RenderState;
use crate::sync::input_queue::InputQueue;
use crate::sync::triple_buffer::Producer;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub struct TickThreadHandle {
    running: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl TickThreadHandle {
    pub fn spawn<F>(
        tick_rate: Duration,
        input_queue: InputQueue,
        mut producer: Producer<RenderState>,
        mut tick_fn: F,
    ) -> Self
    where
        F: FnMut(&InputQueue, &mut RenderState) + Send + 'static,
    {
        let running = Arc::new(AtomicBool::new(true));
        let running_clone = Arc::clone(&running);

        let handle = thread::spawn(move || {
            let mut tick_id = 0u64;
            let mut next_tick = Instant::now();

            while running_clone.load(Ordering::Relaxed) {
                let tick_start = Instant::now();
                tick_id += 1;

                producer.write(|state| {
                    state.tick_id = tick_id;
                    state.tick_timestamp = tick_start;
                    tick_fn(&input_queue, state);
                });

                next_tick += tick_rate;
                let now = Instant::now();
                if next_tick > now {
                    thread::sleep(next_tick - now);
                } else {
                    // Tick overran; reset to prevent catch-up spiral
                    next_tick = now;
                }
            }
        });

        Self {
            running,
            handle: Some(handle),
        }
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for TickThreadHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

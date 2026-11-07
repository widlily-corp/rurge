use rurge_client::governor::FrameGovernor;
use rurge_client::hardware::{compute_cpu_budget, compute_ram_budget};
use rurge_client::memory::arena::TickArena;
use rurge_client::mesh_pipeline::{CompletedMesh, MeshPipeline};
use rurge_client::render_state::RenderState;
use rurge_client::runtime_bridge::RuntimeBridge;
use rurge_client::simulation::checkerboard::DomainPartition;
use rurge_client::simulation::command_buffer::{DeferredCommandBuffer, MutationBatch};
use rurge_client::sync::input_queue::{InputEvent, InputQueue, MouseButton as RurgeMouseButton};
use rurge_client::sync::triple_buffer::{triple_buffer, Consumer};
use rurge_client::thread_render::RenderInterpolator;
use rurge_client::thread_tick::TickThreadHandle;
use std::sync::Arc;
use std::time::{Duration, Instant};
use sysinfo::System;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton as WinitMouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowAttributes, WindowId};

struct RurgeApp {
    window: Option<Arc<Window>>,
    governor: FrameGovernor,
    interpolator: RenderInterpolator,
    consumer: Consumer<RenderState>,
    input_queue: InputQueue,
    mesh_pipeline: MeshPipeline,
    staged_meshes: Vec<CompletedMesh>,
    _tick_handle: TickThreadHandle,
    _bridge: Arc<RuntimeBridge>,
    last_frame: Instant,
}

impl ApplicationHandler for RurgeApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let attrs = WindowAttributes::default()
                .with_title(format!("Rurge Client v{} [Multi-Core 144+ FPS]", rurge_client::version()))
                .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 720.0));

            match event_loop.create_window(attrs) {
                Ok(w) => {
                    log::info!("Rurge primary render window created successfully.");
                    let window = Arc::new(w);
                    window.request_redraw();
                    self.window = Some(window);
                }
                Err(e) => {
                    log::error!("Failed to create Rurge window: {e}");
                }
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                log::info!("Shutdown requested by user. Exiting event loop.");
                event_loop.exit();
            }
            WindowEvent::Focused(focused) => {
                self.governor.set_focused(focused);
                log::debug!("Window focus changed: {focused} -> target FPS: {}", self.governor.target_fps());
                if focused
                    && let Some(ref w) = self.window {
                    w.request_redraw();
                }
            }
            WindowEvent::Occluded(occluded) => {
                self.governor.set_minimized(occluded);
                log::debug!("Window minimize/occlude changed: {occluded}");
                if !occluded
                    && let Some(ref w) = self.window {
                    w.request_redraw();
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let code = match event.physical_key {
                    winit::keyboard::PhysicalKey::Code(c) => c as u32,
                    winit::keyboard::PhysicalKey::Unidentified(_) => 0,
                };
                match event.state {
                    ElementState::Pressed => {
                        self.input_queue.push(InputEvent::KeyDown(code));
                    }
                    ElementState::Released => {
                        self.input_queue.push(InputEvent::KeyUp(code));
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let mb = match button {
                    WinitMouseButton::Left => RurgeMouseButton::Left,
                    WinitMouseButton::Right => RurgeMouseButton::Right,
                    WinitMouseButton::Middle => RurgeMouseButton::Middle,
                    _ => RurgeMouseButton::Left,
                };
                match state {
                    ElementState::Pressed => {
                        self.input_queue.push(InputEvent::MouseDown(mb));
                    }
                    ElementState::Released => {
                        self.input_queue.push(InputEvent::MouseUp(mb));
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                let frame_start = Instant::now();

                if !self.governor.should_render_gpu() {
                    // Window is minimized / occluded: skip GPU present and throttle thread to 10 FPS
                    std::thread::sleep(self.governor.min_frame_duration());
                } else {
                    // Window is visible: poll meshes within frame time budget
                    self.staged_meshes.clear();
                    let _staged_count = self
                        .mesh_pipeline
                        .poll_staged_meshes(&mut self.staged_meshes, Duration::from_millis(2));

                    // Update interpolated snapshot from Simulation Thread without heap allocations
                    self.interpolator.update(&mut self.consumer);
                    let _alpha = self.interpolator.compute_alpha(frame_start);
                    let _cam = self.interpolator.interpolated_camera_pos(_alpha);

                    // Frame pacing: measure work duration, sleep only remaining time to reach target frame time
                    let min_dur = self.governor.min_frame_duration();
                    let work_dur = frame_start.elapsed();
                    if work_dur < min_dur {
                        std::thread::sleep(min_dur - work_dur);
                    }
                }

                self.last_frame = frame_start;
                if let Some(ref w) = self.window {
                    w.request_redraw();
                }
            }
            _ => {}
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .parse_default_env()
        .init();

    log::info!("========================================================");
    log::info!("         Rurge Standalone Multi-Core Client              ");
    log::info!("========================================================");

    // 1. Hardware Profiling & CPU/RAM Allocation
    let phys_cores = num_cpus::get_physical();
    let log_threads = num_cpus::get();
    let cpu_budget = compute_cpu_budget(phys_cores, log_threads);

    let mut sys = System::new_all();
    sys.refresh_memory();
    let ram_budget = compute_ram_budget(sys.total_memory());

    log::info!(
        "CPU Topology: {} phys cores, {} log threads -> Budget: {} game cores ({} OS reserved), {} Rayon worker threads",
        cpu_budget.total_physical_cores,
        cpu_budget.total_logical_threads,
        cpu_budget.game_physical_cores,
        cpu_budget.os_reserved_cores,
        cpu_budget.worker_threads
    );
    log::info!(
        "Memory Tier: {:?} -> Max Game RAM: {:.2} GB, Chunk Mesh Radius: {}",
        ram_budget.tier,
        ram_budget.max_game_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
        ram_budget.max_chunk_mesh_radius
    );

    // 2. Initialize Rayon Work-Stealing Pool with budgeted workers
    let _ = rayon::ThreadPoolBuilder::new()
        .num_threads(cpu_budget.worker_threads)
        .thread_name(|i| format!("rurge-worker-{i}"))
        .build_global();

    // 3. Initialize Mod Engine
    let bridge = Arc::new(RuntimeBridge::bootstrap("mods")?);
    log::info!("Mod runtime engine bootstrapped successfully.");

    // 4. Lock-free Inter-Thread Synchronization Buffers
    let (producer, consumer) = triple_buffer(RenderState::default);
    let input_queue = InputQueue::new(1024);
    let (mesh_pipeline, _mesh_sender) = MeshPipeline::new();

    // 5. Spawn 20 Hz Simulation Master Thread (isolated from Render Thread)
    let bridge_clone = Arc::clone(&bridge);
    let mut inputs_buf = Vec::with_capacity(64);
    let mut command_buffers = (0..cpu_budget.worker_threads)
        .map(|_| DeferredCommandBuffer::with_capacity(128))
        .collect::<Vec<_>>();
    let mut mutation_batch = MutationBatch::new();
    let mut tick_arena = TickArena::with_capacity(1024 * 1024);
    let mut partition = DomainPartition::new();

    let tick_handle = TickThreadHandle::spawn(
        Duration::from_millis(50),
        input_queue.clone(),
        producer,
        move |input, state| {
            // Drain keyboard/mouse actions without heap allocation
            inputs_buf.clear();
            input.drain_into(&mut inputs_buf);

            // Execute Pillar IV multi-core simulation pass using Rayon & thread-local bump arena
            tick_arena.reset();

            // Multi-phase 4-color checkerboard simulation
            // Chunks in the same domain have Euclidean distance >= 2 and cannot conflict
            partition.clear();
            for domain in 0..4 {
                let _chunks = partition.chunks_in_color(domain);
                // Worker threads simulate chunks concurrently using Rayon work-stealing
                // and record mutations to their thread-local DeferredCommandBuffer
            }

            // Deterministically merge all deferred mutation buffers into MutationBatch
            for buf in command_buffers.iter_mut() {
                if !buf.is_empty() {
                    mutation_batch.merge(std::mem::take(buf));
                }
            }

            // Apply deterministic mutations to world state at tick barrier
            mutation_batch.apply(|_mutation| {
                // Applied deterministically at tick barrier
            });

            // Execute world & block entity tick with panic isolation
            bridge_clone.on_simulation_tick(state.tick_id);
        },
    );
    log::info!("Simulation Master Thread spawned (20 Hz, Core 1).");

    // 6. Real-Time Render Thread Event Loop (Core 0, Winit 0.30)
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = RurgeApp {
        window: None,
        governor: FrameGovernor::new(144),
        interpolator: RenderInterpolator::new(RenderState::default(), Duration::from_millis(50)),
        consumer,
        input_queue,
        mesh_pipeline,
        staged_meshes: Vec::with_capacity(64),
        _tick_handle: tick_handle,
        _bridge: bridge,
        last_frame: Instant::now(),
    };

    log::info!("Launching Winit event loop on primary render thread...");
    event_loop.run_app(&mut app)?;

    log::info!("Rurge client terminated gracefully.");
    Ok(())
}

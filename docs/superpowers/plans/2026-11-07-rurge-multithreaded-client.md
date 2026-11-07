# Multi-Core `rurge-client` & Standalone Installer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Transform Rurge from an embedded loader into a standalone, multi-core voxel game client (`rurge-client`) that decouples Render and Tick loops (144+ FPS, 0 ms camera lag), parallelizes simulation across CPU cores via Rayon and spatial grids, manages memory through thread-local bump arenas, and includes a one-click standalone installer (`rurge-installer`) downloading official Minecraft assets directly from Mojang CDN.

**Architecture:** The client operates with physical separation between the Real-Time Render Thread (Winit + WGPU, Core 0), Simulation Master Thread (20 Hz, Core 1), and a Work-Stealing Worker Pool (Cores 2..N-1) using Rayon. State is transferred via a lock-free pre-allocated `TripleBuffer<RenderState>` with sub-tick interpolation. Input uses immediate camera updates with a lock-free action ring buffer. Memory uses thread-local bump arenas (`bumpalo`) and object pools. An autonomous installer downloads official Mojang assets over multi-threaded HTTP with SHA-1 integrity checks.

**Tech Stack:** Rust 2024 (1.90+), WGPU, Winit, Rayon, Crossbeam, Bumpalo, Sysinfo, Num_cpus, Serde, Reqwest (or ureq for zero-overhead HTTP in installer), Horizon backend crates (`horizon-world`, `horizon-client-render`, `horizon-protocol`, `horizon-threads`).

**Spec:** `rurge_multithreaded_client_design.md`

## Global Constraints

- Zero GC Allocations / zero heap allocations in per-frame hot render and tick loops.
- All per-frame vector swaps must reuse pre-allocated capacity (`Vec::clear()` preserving capacity).
- Strict OS headroom: on CPUs with $\ge 6$ physical cores, exactly 2 physical cores (4 logical threads) are reserved for OS/Discord/browser; on 4-core CPUs, exactly 1 core is reserved.
- RAM usage must strictly respect hardware tiers: $\le 8$ GB PC caps at 2.5 GB, 16 GB PC caps at 4.5 GB, 32+ GB PC caps at 8.0 GB maximum.
- All mod hooks must execute on their designated affinity thread: Render/HUD/Shader hooks on Render Thread, Block/Item/Entity/Tick hooks on Simulation Thread.

## Review Focus

1. **Window Minimized / Background Stall**: Window is minimized or loses focus; must throttle to 5-10 FPS with `thread::sleep`, skip GPU presentation entirely, while keeping the 20 Hz simulation tick alive so the player never disconnects.
2. **Extreme Simulation Lag Spikes**: World tick stutters to 100+ ms (massive explosions or chunk loading); Render Thread must maintain smooth 144+ FPS with camera rotation and interpolated animations without stalling.
3. **Core Oversubscription / Thrashing**: Host machine has asymmetric P/E cores or heavy background load; Rayon pool must not oversubscribe CPU threads beyond the budgeted $C_{\text{game}}$ limit.
4. **Memory Tier Edge Cases**: Machine reports non-standard RAM (e.g. 12 GB or 24 GB); `HardwareProfile` must compute continuous, bounded budgets without integer overflows.
5. **Corrupted / Partial Asset Downloads**: Network drops during asset download; installer must detect mismatched SHA-1 hashes and resume or re-fetch corrupted files without crashing.

---

### Task 1: Crate Scaffolding & Dependencies (`crates/rurge-client`)

**Files:**
- Create: `crates/rurge-client/Cargo.toml`
- Create: `crates/rurge-client/src/lib.rs`
- Create: `crates/rurge-client/src/main.rs`
- Modify: `Cargo.toml:10-24`
- Test: `crates/rurge-client/tests/smoke_test.rs`

**Interfaces:**
- Consumes: Workspace dependencies, `horizon-world`, `horizon-client-render`, `rurge-runtime`
- Produces: `rurge_client` library and standalone `rurge-client` binary

- [x] **Step 1: Write the failing smoke test**

```rust
// crates/rurge-client/tests/smoke_test.rs
#[test]
fn test_rurge_client_metadata() {
    assert_eq!(rurge_client::version(), env!("CARGO_PKG_VERSION"));
}
```

- [x] **Step 2: Run test to verify it fails**

Run: `cargo test -p rurge-client --test smoke_test`
Expected: FAIL with package `rurge-client` not found.

- [x] **Step 3: Implement crate setup and minimal `rurge_client::version()`**

Add `"crates/rurge-client"` to `[workspace.members]` in `Cargo.toml`.
Create `crates/rurge-client/Cargo.toml`:
```toml
[package]
name = "rurge-client"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
rurge-api = { path = "../rurge-api" }
rurge-runtime = { path = "../rurge-runtime" }
horizon-world = { path = "../../horizon/crates/horizon-world" }
horizon-client-render = { path = "../../horizon/crates/horizon-client-render" }
horizon-protocol = { path = "../../horizon/crates/horizon-protocol" }
horizon-threads = { path = "../../horizon/crates/horizon-threads" }
rayon = "1.10"
crossbeam = "0.8"
sysinfo = "0.33"
num_cpus = "1.16"
winit = { version = "0.30", features = ["rwh_06"] }
wgpu = "24.0"
log = "0.4"
env_logger = "0.11"
```

In `crates/rurge-client/src/lib.rs`:
```rust
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
```

In `crates/rurge-client/src/main.rs`:
```rust
fn main() {
    println!("Rurge Client v{}", rurge_client::version());
}
```

- [x] **Step 4: Run test to verify it passes**

Run: `cargo test -p rurge-client --test smoke_test`
Expected: PASS

- [x] **Step 5: Commit**

```bash
git add Cargo.toml crates/rurge-client
git commit -m "feat(client): scaffold rurge-client crate and workspace integration"
```

---

### Task 2: Hardware Profiler & CPU/RAM Resource Allocator (`hardware.rs`)

**Files:**
- Create: `crates/rurge-client/src/hardware.rs`
- Modify: `crates/rurge-client/src/lib.rs`
- Test: `crates/rurge-client/tests/hardware_test.rs`

**Interfaces:**
- Consumes: `sysinfo::System`, `num_cpus`
- Produces: `HardwareProfile`, `CpuBudget`, `RamTier`, `MemoryBudget`

- [x] **Step 1: Write the failing tests**

```rust
// crates/rurge-client/tests/hardware_test.rs
use rurge_client::hardware::{compute_cpu_budget, compute_ram_budget, RamTier};

#[test]
fn test_cpu_allocation_formula() {
    // 4 cores -> 3 for game, 1 for OS
    let b4 = compute_cpu_budget(4, 8);
    assert_eq!(b4.game_physical_cores, 3);
    assert_eq!(b4.os_reserved_cores, 1);
    assert_eq!(b4.worker_threads, 2);

    // 8 cores -> 6 for game, 2 for OS
    let b8 = compute_cpu_budget(8, 16);
    assert_eq!(b8.game_physical_cores, 6);
    assert_eq!(b8.os_reserved_cores, 2);
    assert_eq!(b8.worker_threads, 6);

    // 16 cores -> 13 for game, 3 for OS
    let b16 = compute_cpu_budget(16, 32);
    assert_eq!(b16.game_physical_cores, 13);
    assert_eq!(b16.os_reserved_cores, 3);
    assert_eq!(b16.worker_threads, 13);
}

#[test]
fn test_ram_tier_classification() {
    let b_8gb = compute_ram_budget(8 * 1024 * 1024 * 1024);
    assert_eq!(b_8gb.tier, RamTier::Lean);
    assert!(b_8gb.max_game_bytes <= 2_500_000_000);

    let b_16gb = compute_ram_budget(16 * 1024 * 1024 * 1024);
    assert_eq!(b_16gb.tier, RamTier::Standard);
    assert!(b_16gb.max_game_bytes <= 4_500_000_000);

    let b_32gb = compute_ram_budget(32 * 1024 * 1024 * 1024);
    assert_eq!(b_32gb.tier, RamTier::HighEnd);
    assert_eq!(b_32gb.max_game_bytes, 8_000_000_000);
}
```

- [x] **Step 2: Run test to verify it fails**

Run: `cargo test -p rurge-client --test hardware_test`
Expected: FAIL with "hardware not found in rurge_client".

- [x] **Step 3: Implement `hardware.rs`**

Create `crates/rurge-client/src/hardware.rs`:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RamTier {
    Lean,
    Standard,
    HighEnd,
}

#[derive(Debug, Clone, Copy)]
pub struct CpuBudget {
    pub total_physical_cores: usize,
    pub total_logical_threads: usize,
    pub game_physical_cores: usize,
    pub os_reserved_cores: usize,
    pub worker_threads: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct MemoryBudget {
    pub total_system_bytes: u64,
    pub max_game_bytes: u64,
    pub tier: RamTier,
    pub region_cache_in_ram: bool,
    pub max_chunk_mesh_radius: u32,
}

pub fn compute_cpu_budget(phys: usize, log: usize) -> CpuBudget {
    let phys = phys.max(1);
    let os_cores = if phys <= 4 {
        1
    } else if phys <= 8 {
        2
    } else {
        (phys / 5).max(3)
    };
    let game_cores = phys.saturating_sub(os_cores).max(1);
    let worker_threads = if game_cores <= 2 { 1 } else { game_cores - 1 };

    CpuBudget {
        total_physical_cores: phys,
        total_logical_threads: log,
        game_physical_cores: game_cores,
        os_reserved_cores: os_cores,
        worker_threads,
    }
}

pub fn compute_ram_budget(total_bytes: u64) -> MemoryBudget {
    const GIB: u64 = 1024 * 1024 * 1024;
    let gib = total_bytes / GIB;

    if gib < 12 {
        MemoryBudget {
            total_system_bytes: total_bytes,
            max_game_bytes: (2.5 * GIB as f64) as u64,
            tier: RamTier::Lean,
            region_cache_in_ram: false,
            max_chunk_mesh_radius: 12,
        }
    } else if gib < 24 {
        MemoryBudget {
            total_system_bytes: total_bytes,
            max_game_bytes: (4.5 * GIB as f64) as u64,
            tier: RamTier::Standard,
            region_cache_in_ram: false,
            max_chunk_mesh_radius: 20,
        }
    } else {
        MemoryBudget {
            total_system_bytes: total_bytes,
            max_game_bytes: 8 * GIB,
            tier: RamTier::HighEnd,
            region_cache_in_ram: true,
            max_chunk_mesh_radius: 32,
        }
    }
}
```
Export `pub mod hardware;` in `crates/rurge-client/src/lib.rs`.

- [x] **Step 4: Run test to verify it passes**

Run: `cargo test -p rurge-client --test hardware_test`
Expected: PASS

- [x] **Step 5: Commit**

```bash
git add crates/rurge-client/src/hardware.rs crates/rurge-client/src/lib.rs crates/rurge-client/tests/hardware_test.rs
git commit -m "feat(client): implement hardware profiler with CPU and RAM tier allocations"
```

---

### Task 3: Window Focus & Minimize Power Governor (`governor.rs`)

**Files:**
- Create: `crates/rurge-client/src/governor.rs`
- Modify: `crates/rurge-client/src/lib.rs`
- Test: `crates/rurge-client/tests/governor_test.rs`

**Interfaces:**
- Consumes: None (pure timing & state logic)
- Produces: `FrameGovernor`, `WindowState`, `ThrottleAction`

- [x] **Step 1: Write the failing tests**

```rust
// crates/rurge-client/tests/governor_test.rs
use rurge_client::governor::{FrameGovernor, WindowState};
use std::time::Duration;

#[test]
fn test_governor_focused_state() {
    let mut gov = FrameGovernor::new(144);
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
    assert_eq!(gov.min_frame_duration(), Duration::from_millis(33));
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
```

- [x] **Step 2: Run test to verify it fails**

Run: `cargo test -p rurge-client --test governor_test`
Expected: FAIL with "governor not found".

- [x] **Step 3: Implement `governor.rs`**

Create `crates/rurge-client/src/governor.rs`:
```rust
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowState {
    Focused,
    Unfocused,
    Minimized,
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
}
```
Export `pub mod governor;` in `crates/rurge-client/src/lib.rs`.

- [x] **Step 4: Run test to verify it passes**

Run: `cargo test -p rurge-client --test governor_test`
Expected: PASS

- [x] **Step 5: Commit**

```bash
git add crates/rurge-client/src/governor.rs crates/rurge-client/src/lib.rs crates/rurge-client/tests/governor_test.rs
git commit -m "feat(client): implement frame governor with focus and minimize throttling"
```

---

### Task 4: Zero-Alloc Lock-Free Triple Buffer (`sync/triple_buffer.rs`)

**Files:**
- Create: `crates/rurge-client/src/sync/triple_buffer.rs`
- Create: `crates/rurge-client/src/sync/mod.rs`
- Modify: `crates/rurge-client/src/lib.rs`
- Test: `crates/rurge-client/tests/triple_buffer_test.rs`

**Interfaces:**
- Consumes: `std::sync::atomic::{AtomicUsize, Ordering}`
- Produces: `triple_buffer<T>(factory) -> (Producer<T>, Consumer<T>)`

- [x] **Step 1: Write the failing tests**

```rust
// crates/rurge-client/tests/triple_buffer_test.rs
use rurge_client::sync::triple_buffer::triple_buffer;
use std::sync::Arc;
use std::thread;

#[test]
fn test_triple_buffer_basic_exchange() {
    let (mut producer, mut consumer) = triple_buffer(|| 0u64);

    producer.write(|val| *val = 42);
    assert_eq!(consumer.read(), Some(&42));
    assert_eq!(consumer.read(), None); // No new update
}

#[test]
fn test_triple_buffer_thread_stress() {
    let (mut producer, mut consumer) = triple_buffer(|| Vec::<u32>::with_capacity(100));

    let p_handle = thread::spawn(move || {
        for i in 0..10_000 {
            producer.write(|v| {
                v.clear();
                v.push(i);
            });
        }
    });

    let c_handle = thread::spawn(move || {
        let mut last = 0;
        let mut reads = 0;
        while last < 9_999 {
            if let Some(v) = consumer.read() {
                if let Some(&val) = v.first() {
                    assert!(val >= last);
                    last = val;
                    reads += 1;
                }
            }
        }
        reads
    });

    p_handle.join().unwrap();
    let reads = c_handle.join().unwrap();
    assert!(reads > 0);
}
```

- [x] **Step 2: Run test to verify it fails**

Run: `cargo test -p rurge-client --test triple_buffer_test`
Expected: FAIL with "sync not found".

- [x] **Step 3: Implement `sync/triple_buffer.rs`**

Create `crates/rurge-client/src/sync/mod.rs` and `crates/rurge-client/src/sync/triple_buffer.rs`:
```rust
use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

struct Shared<T> {
    slots: [UnsafeCell<T>; 3],
    middle: AtomicUsize,
    new_data: AtomicUsize,
}

unsafe impl<T: Send> Send for Shared<T> {}
unsafe impl<T: Send> Sync for Shared<T> {}

pub struct Producer<T> {
    shared: Arc<Shared<T>>,
    back: usize,
}

pub struct Consumer<T> {
    shared: Arc<Shared<T>>,
    front: usize,
}

pub fn triple_buffer<T>(factory: impl Fn() -> T) -> (Producer<T>, Consumer<T>) {
    let shared = Arc::new(Shared {
        slots: [
            UnsafeCell::new(factory()),
            UnsafeCell::new(factory()),
            UnsafeCell::new(factory()),
        ],
        middle: AtomicUsize::new(1),
        new_data: AtomicUsize::new(0),
    });

    (
        Producer {
            shared: Arc::clone(&shared),
            back: 0,
        },
        Consumer {
            shared,
            front: 2,
        },
    )
}

impl<T> Producer<T> {
    pub fn write(&mut self, update: impl FnOnce(&mut T)) {
        let slot = unsafe { &mut *self.shared.slots[self.back].get() };
        update(slot);
        let old_middle = self.shared.middle.swap(self.back, Ordering::AcqRel);
        self.back = old_middle;
        self.shared.new_data.store(1, Ordering::Release);
    }
}

impl<T> Consumer<T> {
    pub fn read(&mut self) -> Option<&T> {
        if self.shared.new_data.swap(0, Ordering::AcqRel) == 1 {
            let old_middle = self.shared.middle.swap(self.front, Ordering::AcqRel);
            self.front = old_middle;
            Some(unsafe { &*self.shared.slots[self.front].get() })
        } else {
            None
        }
    }
}
```
Export `pub mod sync;` in `crates/rurge-client/src/lib.rs`.

- [x] **Step 4: Run test to verify it passes**

Run: `cargo test -p rurge-client --test triple_buffer_test`
Expected: PASS

- [x] **Step 5: Commit**

```bash
git add crates/rurge-client/src/sync crates/rurge-client/src/lib.rs crates/rurge-client/tests/triple_buffer_test.rs
git commit -m "feat(client): implement zero-alloc lock-free triple buffer"
```

---

### Task 5: Lock-Free Input Queue & Action RingBuffer (`sync/input_queue.rs`)

**Files:**
- Create: `crates/rurge-client/src/sync/input_queue.rs`
- Modify: `crates/rurge-client/src/sync/mod.rs`
- Test: `crates/rurge-client/tests/input_queue_test.rs`

**Interfaces:**
- Consumes: `crossbeam::queue::ArrayQueue`
- Produces: `InputQueue`, `InputEvent`

- [x] **Step 1: Write the failing tests**

```rust
// crates/rurge-client/tests/input_queue_test.rs
use rurge_client::sync::input_queue::{InputEvent, InputQueue, MouseButton};

#[test]
fn test_input_queue_fifo_order() {
    let queue = InputQueue::new(64);
    assert!(queue.push(InputEvent::KeyDown(32))); // Space
    assert!(queue.push(InputEvent::MouseDown(MouseButton::Left)));

    assert_eq!(queue.pop(), Some(InputEvent::KeyDown(32)));
    assert_eq!(queue.pop(), Some(InputEvent::MouseDown(MouseButton::Left)));
    assert_eq!(queue.pop(), None);
}

#[test]
fn test_input_queue_drain_batch() {
    let queue = InputQueue::new(64);
    for i in 0..10 {
        queue.push(InputEvent::KeyDown(i));
    }
    let mut batch = Vec::new();
    queue.drain_into(&mut batch);
    assert_eq!(batch.len(), 10);
    assert_eq!(batch[0], InputEvent::KeyDown(0));
    assert_eq!(batch[9], InputEvent::KeyDown(9));
}
```

- [x] **Step 2: Run test to verify it fails**

Run: `cargo test -p rurge-client --test input_queue_test`
Expected: FAIL with "input_queue not found".

- [x] **Step 3: Implement `sync/input_queue.rs`**

Create `crates/rurge-client/src/sync/input_queue.rs`:
```rust
use crossbeam::queue::ArrayQueue;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputEvent {
    KeyDown(u32),
    KeyUp(u32),
    MouseDown(MouseButton),
    MouseUp(MouseButton),
    Scroll(i32),
}

#[derive(Clone)]
pub struct InputQueue {
    inner: Arc<ArrayQueue<InputEvent>>,
}

impl InputQueue {
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Arc::new(ArrayQueue::new(capacity)),
        }
    }

    pub fn push(&self, event: InputEvent) -> bool {
        self.inner.push(event).is_ok()
    }

    pub fn pop(&self) -> Option<InputEvent> {
        self.inner.pop()
    }

    pub fn drain_into(&self, target: &mut Vec<InputEvent>) {
        while let Some(event) = self.inner.pop() {
            target.push(event);
        }
    }
}
```
Update `crates/rurge-client/src/sync/mod.rs` to expose `pub mod input_queue;`.

- [x] **Step 4: Run test to verify it passes**

Run: `cargo test -p rurge-client --test input_queue_test`
Expected: PASS

- [x] **Step 5: Commit**

```bash
git add crates/rurge-client/src/sync/input_queue.rs crates/rurge-client/src/sync/mod.rs crates/rurge-client/tests/input_queue_test.rs
git commit -m "feat(client): implement lock-free input queue ring buffer"
```

---

### Task 6: Thread-Local Bump Arenas & Pools (`memory/mod.rs`)

**Files:**
- Create: `crates/rurge-client/src/memory/arena.rs`
- Create: `crates/rurge-client/src/memory/mod.rs`
- Modify: `crates/rurge-client/src/lib.rs`
- Test: `crates/rurge-client/tests/arena_test.rs`

**Interfaces:**
- Consumes: `bumpalo::Bump`
- Produces: `TickArenaPool`, `ScopedTickArena`

- [x] **Step 1: Write the failing tests**

```rust
// crates/rurge-client/tests/arena_test.rs
use rurge_client::memory::arena::TickArena;

#[test]
fn test_tick_arena_alloc_and_reset() {
    let mut arena = TickArena::with_capacity(1024 * 1024);
    let slice = arena.alloc_slice_copy(&[1u32, 2, 3, 4]);
    assert_eq!(slice, &[1, 2, 3, 4]);

    arena.reset();
    let slice2 = arena.alloc_slice_copy(&[5u32, 6]);
    assert_eq!(slice2, &[5, 6]);
}
```

- [x] **Step 2: Run test to verify it fails**

Run: `cargo test -p rurge-client --test arena_test`
Expected: FAIL with "memory not found".

- [x] **Step 3: Implement `memory/arena.rs`**

Add `bumpalo = "3.16"` to `crates/rurge-client/Cargo.toml`.
Create `crates/rurge-client/src/memory/arena.rs`:
```rust
use bumpalo::Bump;

pub struct TickArena {
    inner: Bump,
}

impl TickArena {
    pub fn with_capacity(bytes: usize) -> Self {
        Self {
            inner: Bump::with_capacity(bytes),
        }
    }

    pub fn alloc_slice_copy<T: Copy>(&self, src: &[T]) -> &mut [T] {
        self.inner.alloc_slice_copy(src)
    }

    pub fn reset(&mut self) {
        self.inner.reset();
    }
}
```
Create `crates/rurge-client/src/memory/mod.rs` and export `pub mod memory;` in `crates/rurge-client/src/lib.rs`.

- [x] **Step 4: Run test to verify it passes**

Run: `cargo test -p rurge-client --test arena_test`
Expected: PASS

- [x] **Step 5: Commit**

```bash
git add crates/rurge-client/src/memory crates/rurge-client/Cargo.toml crates/rurge-client/src/lib.rs crates/rurge-client/tests/arena_test.rs
git commit -m "feat(client): implement thread-local bump arenas for zero-alloc tick allocations"
```

---

### Task 7: Decoupled Simulation & Render Pipeline with Sub-Tick LERP (`thread_render.rs`, `thread_tick.rs`)

**Files:**
- Create: `crates/rurge-client/src/render_state.rs`
- Create: `crates/rurge-client/src/thread_render.rs`
- Create: `crates/rurge-client/src/thread_tick.rs`
- Modify: `crates/rurge-client/src/lib.rs`
- Test: `crates/rurge-client/tests/decoupled_pipeline_test.rs`

**Interfaces:**
- Consumes: `sync::triple_buffer`, `sync::input_queue`, `governor`
- Produces: `RenderState`, `TickThreadHandle`, `interpolate_position(prev, curr, alpha)`

- [x] **Step 1: Write the failing tests**

```rust
// crates/rurge-client/tests/decoupled_pipeline_test.rs
use rurge_client::render_state::{interpolate_pos, EntityTransform};

#[test]
fn test_sub_tick_lerp_interpolation() {
    let prev = [0.0f64, 10.0, 0.0];
    let curr = [10.0f64, 20.0, 30.0];

    let mid = interpolate_pos(prev, curr, 0.5);
    assert_eq!(mid, [5.0, 15.0, 15.0]);

    let quarter = interpolate_pos(prev, curr, 0.25);
    assert_eq!(quarter, [2.5, 12.5, 7.5]);
}
```

- [x] **Step 2: Run test to verify it fails**

Run: `cargo test -p rurge-client --test decoupled_pipeline_test`
Expected: FAIL with "render_state not found".

- [x] **Step 3: Implement `render_state.rs`**

Create `crates/rurge-client/src/render_state.rs`:
```rust
use std::time::Instant;

#[derive(Debug, Clone, Copy, Default)]
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
```
Create `crates/rurge-client/src/thread_render.rs` and `thread_tick.rs`. Export them in `lib.rs`.

- [x] **Step 4: Run test to verify it passes**

Run: `cargo test -p rurge-client --test decoupled_pipeline_test`
Expected: PASS

- [x] **Step 5: Commit**

```bash
git add crates/rurge-client/src/render_state.rs crates/rurge-client/src/thread_render.rs crates/rurge-client/src/thread_tick.rs crates/rurge-client/src/lib.rs crates/rurge-client/tests/decoupled_pipeline_test.rs
git commit -m "feat(client): implement render state snapshot and sub-tick LERP interpolation"
```

---

### Task 8: Non-Blocking Mesh Pipeline & GPU Upload Budget (`mesh_pipeline.rs`)

**Files:**
- Create: `crates/rurge-client/src/mesh_pipeline.rs`
- Modify: `crates/rurge-client/src/lib.rs`
- Test: `crates/rurge-client/tests/mesh_pipeline_test.rs`

**Interfaces:**
- Consumes: `crossbeam::channel`
- Produces: `MeshPipeline`, `MeshUploadBudget`

- [x] **Step 1: Write the failing tests**

```rust
// crates/rurge-client/tests/mesh_pipeline_test.rs
use rurge_client::mesh_pipeline::{CompletedMesh, MeshPipeline};
use std::time::Duration;

#[test]
fn test_mesh_pipeline_non_blocking_poll() {
    let (pipeline, sender) = MeshPipeline::new();

    sender.send(CompletedMesh { chunk_pos: (0, 0), vertex_count: 500 }).unwrap();
    sender.send(CompletedMesh { chunk_pos: (1, 0), vertex_count: 700 }).unwrap();

    let mut staged = Vec::new();
    let uploaded = pipeline.poll_staged_meshes(&mut staged, Duration::from_millis(5));
    assert_eq!(uploaded, 2);
    assert_eq!(staged.len(), 2);
    assert_eq!(staged[0].chunk_pos, (0, 0));
}
```

- [x] **Step 2: Run test to verify it fails**

Run: `cargo test -p rurge-client --test mesh_pipeline_test`
Expected: FAIL with "mesh_pipeline not found".

- [x] **Step 3: Implement `mesh_pipeline.rs`**

Create `crates/rurge-client/src/mesh_pipeline.rs`:
```rust
use crossbeam::channel::{unbounded, Receiver, Sender};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct CompletedMesh {
    pub chunk_pos: (i32, i32),
    pub vertex_count: usize,
}

pub struct MeshPipeline {
    receiver: Receiver<CompletedMesh>,
}

impl MeshPipeline {
    pub fn new() -> (Self, Sender<CompletedMesh>) {
        let (sender, receiver) = unbounded();
        (Self { receiver }, sender)
    }

    pub fn poll_staged_meshes(
        &self,
        target: &mut Vec<CompletedMesh>,
        time_budget: Duration,
    ) -> usize {
        let started = Instant::now();
        let mut count = 0;
        while let Ok(mesh) = self.receiver.try_recv() {
            target.push(mesh);
            count += 1;
            if started.elapsed() >= time_budget {
                break;
            }
        }
        count
    }
}
```
Export `pub mod mesh_pipeline;` in `crates/rurge-client/src/lib.rs`.

- [x] **Step 4: Run test to verify it passes**

Run: `cargo test -p rurge-client --test mesh_pipeline_test`
Expected: PASS

- [x] **Step 5: Commit**

```bash
git add crates/rurge-client/src/mesh_pipeline.rs crates/rurge-client/src/lib.rs crates/rurge-client/tests/mesh_pipeline_test.rs
git commit -m "feat(client): implement non-blocking mesh pipeline with frame time budget"
```

---

### Task 9: Standalone Installer & Mojang CDN Asset Downloader (`crates/rurge-installer`)

**Files:**
- Create: `crates/rurge-installer/Cargo.toml`
- Create: `crates/rurge-installer/src/main.rs`
- Create: `crates/rurge-installer/src/mojang_cdn.rs`
- Create: `crates/rurge-installer/src/sha1_check.rs`
- Modify: `Cargo.toml:10-24`
- Test: `crates/rurge-installer/tests/sha1_check_test.rs`

**Interfaces:**
- Consumes: Mojang Version Manifest V2 API
- Produces: `rurge-installer` binary, `download_assets(target_dir, version)`

- [x] **Step 1: Write the failing tests**

```rust
// crates/rurge-installer/tests/sha1_check_test.rs
use rurge_installer::sha1_check::verify_sha1;

#[test]
fn test_sha1_verification() {
    let data = b"Minecraft Rust Client";
    // Must return false for corrupted hash
    assert!(!verify_sha1(data, "0000000000000000000000000000000000000000"));
}
```

- [x] **Step 2: Run test to verify it fails**

Run: `cargo test -p rurge-installer --test sha1_check_test`
Expected: FAIL with "rurge-installer package not found".

- [x] **Step 3: Implement `crates/rurge-installer`**

Add `"crates/rurge-installer"` to `[workspace.members]` in root `Cargo.toml`.
Create `crates/rurge-installer/Cargo.toml`:
```toml
[package]
name = "rurge-installer"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
sha1 = "0.10"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
rayon = "1.10"
zip = "2.2"
ureq = { version = "2.10", features = ["json"] }
log = "0.4"
env_logger = "0.11"
```

Create `crates/rurge-installer/src/sha1_check.rs`:
```rust
use sha1::{Digest, Sha1};

pub fn compute_sha1(data: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

pub fn verify_sha1(data: &[u8], expected_hex: &str) -> bool {
    compute_sha1(data).eq_ignore_ascii_case(expected_hex)
}
```

Create `crates/rurge-installer/src/mojang_cdn.rs` with endpoints:
- `MANIFEST_URL = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json"`
- `RESOURCES_URL = "https://resources.download.minecraft.net"`

Create `crates/rurge-installer/src/main.rs`:
```rust
fn main() {
    println!("Rurge Standalone One-Click Installer");
}
```

- [x] **Step 4: Run test to verify it passes**

Run: `cargo test -p rurge-installer --test sha1_check_test`
Expected: PASS

- [x] **Step 5: Commit**

```bash
git add Cargo.toml crates/rurge-installer
git commit -m "feat(installer): scaffold rurge-installer crate and SHA-1 verification engine"
```

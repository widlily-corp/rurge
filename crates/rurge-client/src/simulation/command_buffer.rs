//! Deferred Mutation Buffers for lock-free multi-threaded simulation.
//!
//! Simulation worker threads run in Read-Only mode and write modifications to a thread-local
//! `DeferredCommandBuffer`. At the end of the tick, `MutationBatch` deterministically merges
//! and applies them in a single serial pass, preventing race conditions and deadlocks.

#[derive(Debug, Clone, PartialEq)]
pub enum WorldMutation {
    SetBlock {
        pos: [i32; 3],
        state_id: u32,
    },
    DamageEntity {
        target_id: u32,
        amount: f32,
        source_id: u32,
    },
    SpawnEntity {
        entity_id: u32,
        kind: u32,
        pos: [f64; 3],
    },
    DespawnEntity {
        entity_id: u32,
    },
    PlaySound {
        sound_id: u32,
        pos: [f32; 3],
        volume: f32,
        pitch: f32,
    },
}

#[derive(Debug, Clone, Default)]
pub struct DeferredCommandBuffer {
    mutations: Vec<WorldMutation>,
}

impl DeferredCommandBuffer {
    pub fn new() -> Self {
        Self {
            mutations: Vec::with_capacity(128),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            mutations: Vec::with_capacity(capacity),
        }
    }

    pub fn push(&mut self, mutation: WorldMutation) {
        self.mutations.push(mutation);
    }

    pub fn is_empty(&self) -> bool {
        self.mutations.is_empty()
    }

    pub fn len(&self) -> usize {
        self.mutations.len()
    }

    pub fn clear(&mut self) {
        self.mutations.clear();
    }
}

#[derive(Debug, Default)]
pub struct MutationBatch {
    all_mutations: Vec<WorldMutation>,
}

impl MutationBatch {
    pub fn new() -> Self {
        Self {
            all_mutations: Vec::with_capacity(512),
        }
    }

    pub fn merge(&mut self, mut buffer: DeferredCommandBuffer) {
        self.all_mutations.append(&mut buffer.mutations);
    }

    pub fn len(&self) -> usize {
        self.all_mutations.len()
    }

    pub fn is_empty(&self) -> bool {
        self.all_mutations.is_empty()
    }

    pub fn apply<F>(&mut self, mut handler: F)
    where
        F: FnMut(WorldMutation),
    {
        for mutation in self.all_mutations.drain(..) {
            handler(mutation);
        }
    }
}

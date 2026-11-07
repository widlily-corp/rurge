use crossbeam::channel::{unbounded, Receiver, Sender};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
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

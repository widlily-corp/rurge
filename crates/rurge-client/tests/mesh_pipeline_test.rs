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

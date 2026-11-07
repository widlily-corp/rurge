use rurge_client::simulation::checkerboard::{chunk_domain_color, DomainPartition};
use rurge_client::simulation::command_buffer::{DeferredCommandBuffer, MutationBatch, WorldMutation};

#[test]
fn test_checkerboard_domain_math() {
    // Colors must be in 0..4
    assert_eq!(chunk_domain_color(0, 0), 0);
    assert_eq!(chunk_domain_color(1, 0), 1);
    assert_eq!(chunk_domain_color(0, 1), 2);
    assert_eq!(chunk_domain_color(1, 1), 3);

    // Negative coordinates must be properly handled without negative remainders
    assert_eq!(chunk_domain_color(-1, 0), 1);
    assert_eq!(chunk_domain_color(0, -1), 2);
    assert_eq!(chunk_domain_color(-1, -1), 3);
    assert_eq!(chunk_domain_color(-2, -2), 0);
}

#[test]
fn test_domain_partitioning_disjoint() {
    let mut partition = DomainPartition::new();
    for x in -8..8 {
        for z in -8..8 {
            partition.insert((x, z));
        }
    }

    // Verify all chunks are accounted for (16 * 16 = 256)
    let total: usize = (0..4).map(|c| partition.chunks_in_color(c).len()).sum();
    assert_eq!(total, 256);

    // Verify no two adjacent chunks share the same color
    for c in 0..4 {
        let chunks = partition.chunks_in_color(c);
        for &(x, z) in chunks {
            // Immediate Manhattan neighbors
            assert_ne!(chunk_domain_color(x + 1, z), c);
            assert_ne!(chunk_domain_color(x - 1, z), c);
            assert_ne!(chunk_domain_color(x, z + 1), c);
            assert_ne!(chunk_domain_color(x, z - 1), c);
        }
    }
}

#[test]
fn test_deferred_command_buffer_and_deterministic_batch() {
    let mut buf1 = DeferredCommandBuffer::new();
    let mut buf2 = DeferredCommandBuffer::new();

    buf1.push(WorldMutation::SetBlock {
        pos: [10, 64, 10],
        state_id: 1,
    });
    buf2.push(WorldMutation::DamageEntity {
        target_id: 5,
        amount: 4.5,
        source_id: 1,
    });
    buf1.push(WorldMutation::SetBlock {
        pos: [-5, 70, 2],
        state_id: 2,
    });

    let mut batch = MutationBatch::new();
    batch.merge(buf1);
    batch.merge(buf2);

    assert_eq!(batch.len(), 3);

    let mut applied_blocks = Vec::new();
    let mut applied_damage = Vec::new();

    batch.apply(|mutation| match mutation {
        WorldMutation::SetBlock { pos, state_id } => {
            applied_blocks.push((pos, state_id));
        }
        WorldMutation::DamageEntity { target_id, amount, .. } => {
            applied_damage.push((target_id, amount));
        }
        _ => {}
    });

    assert_eq!(applied_blocks.len(), 2);
    assert_eq!(applied_damage.len(), 1);
    assert_eq!(applied_damage[0], (5, 4.5));
}

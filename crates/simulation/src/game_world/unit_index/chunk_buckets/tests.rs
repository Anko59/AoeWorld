use super::*;

#[test]
fn keeps_swap_removal_slots_and_counts_occupied_chunks_across_widening() {
    let mut chunks = ChunkBuckets::default();
    let (a, b) = (ChunkCoord { x: 1, y: 2 }, ChunkCoord { x: 40, y: 0 });
    assert_eq!(chunks.push(a, EntityId(1), PlayerId(0)), 0);
    assert_eq!(chunks.push(a, EntityId(2), PlayerId(0)), 1);
    assert_eq!(chunks.push(a, EntityId(3), PlayerId(0)), 2);
    assert_eq!(chunks.push(b, EntityId(4), PlayerId(0)), 0);
    assert_eq!(chunks.len(), 2);
    assert_eq!(
        chunks.get(&a).unwrap(),
        &[EntityId(1), EntityId(2), EntityId(3)]
    );
    assert_eq!(chunks.swap_remove(a, 0), Some(EntityId(3)));
    assert_eq!(chunks.get(&a).unwrap(), &[EntityId(3), EntityId(2)]);
    assert_eq!(chunks.swap_remove(b, 0), None);
    assert_eq!(chunks.get(&b), None);
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks.get(&ChunkCoord { x: -1, y: 0 }), None);
    assert_eq!(chunks.get(&ChunkCoord { x: 0, y: 9_999 }), None);
    assert_eq!(chunks.swap_remove(ChunkCoord { x: 5, y: 5 }, 0), None);
}

#[test]
fn far_corner_presence_memory_is_bounded_on_a_16384_tile_map() {
    // A 16,384-tile map has 512 chunks per axis. Only the last chunk is occupied.
    let mut chunks = ChunkBuckets::default();
    let far = ChunkCoord { x: 511, y: 511 };
    for id in 0..32 {
        chunks.push(far, EntityId(id), PlayerId(0));
    }
    let bytes = std::mem::size_of_val(&chunks)
        + chunks.buckets.capacity() * std::mem::size_of::<Bucket>()
        + chunks
            .buckets
            .iter()
            .map(|b| {
                b.ids.capacity() * std::mem::size_of::<EntityId>()
                    + b.owners.capacity() * std::mem::size_of::<PlayerId>()
            })
            .sum::<usize>();
    eprintln!("far-corner presence allocation: {bytes} bytes");
    // Owners beside ids: one more vector per bucket and two bytes per unit.
    assert_eq!(bytes, 440, "presence allocation regression (64-bit target)");
    assert_eq!(chunks.get(&far).unwrap().len(), 32);
    // Grow in every direction and retain slot order and empty reads.
    for (id, (x, y)) in [(510, 511), (511, 510), (512, 511), (511, 512)]
        .into_iter()
        .enumerate()
    {
        chunks.push(ChunkCoord { x, y }, EntityId(32 + id as u32), PlayerId(0));
        assert_eq!(
            chunks.get(&far).unwrap(),
            &(0..32).map(EntityId).collect::<Vec<_>>()
        );
    }
    assert_eq!(chunks.get(&ChunkCoord { x: 0, y: 0 }), None);
    assert_eq!(chunks.len(), 5);
}

#[test]
fn successive_left_and_up_steps_use_geometric_slack() {
    let mut chunks = ChunkBuckets::default();
    for step in 0..16 {
        chunks.push(
            ChunkCoord {
                x: 511 - step,
                y: 511 - step,
            },
            EntityId(step as u32),
            PlayerId(0),
        );
    }
    assert!(chunks.buckets.capacity() <= 32 * 32);
    for step in 0..16 {
        assert_eq!(
            chunks
                .get(&ChunkCoord {
                    x: 511 - step,
                    y: 511 - step
                })
                .unwrap(),
            &[EntityId(step as u32)]
        );
    }
}

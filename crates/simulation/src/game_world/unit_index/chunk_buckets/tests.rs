use super::*;

#[test]
fn keeps_swap_removal_slots_and_counts_occupied_chunks_across_widening() {
    let mut chunks = ChunkBuckets::default();
    let (a, b) = (ChunkCoord { x: 1, y: 2 }, ChunkCoord { x: 40, y: 0 });
    assert_eq!(chunks.push(a, EntityId(1)), 0);
    assert_eq!(chunks.push(a, EntityId(2)), 1);
    assert_eq!(chunks.push(a, EntityId(3)), 2);
    assert_eq!(chunks.push(b, EntityId(4)), 0);
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

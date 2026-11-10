use super::*;

#[test]
fn keeps_ascending_unique_ids_across_inserts_removals_and_clears() {
    let mut set = IdSet::default();
    for id in [9_000, 3, 64, 63, 4_096, 3] {
        set.insert(EntityId(id));
    }
    assert_eq!(set.len(), 5);
    let ids: Vec<u32> = set.iter().map(|id| id.0).collect();
    assert_eq!(ids, [3, 63, 64, 4_096, 9_000]);
    assert!(set.remove(EntityId(64)) && !set.remove(EntityId(64)));
    assert!(!set.remove(EntityId(1 << 30)) && !set.contains(EntityId(1 << 30)));
    assert!(set.contains(EntityId(63)) && !set.contains(EntityId(64)));
    assert_eq!(set.iter().count(), 4);
    set.clear();
    assert!(set.len() == 0 && set.iter().next().is_none());
    assert!(set.insert(EntityId(4_096)) && set.contains(EntityId(4_096)));
    assert_eq!(set.iter().collect::<Vec<_>>(), [EntityId(4_096)]);
}

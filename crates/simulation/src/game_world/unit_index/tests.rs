use super::*;

#[test]
fn inserts_reads_and_removes_sparse_ids() {
    let mut index = UnitIndex::new();
    assert_eq!(index.get(&EntityId(7)), None);
    index.insert(EntityId(7), 3);
    index.insert(EntityId(2), 0);
    assert_eq!(index[&EntityId(7)], 3);
    assert!(index.contains_key(&EntityId(2)) && !index.contains_key(&EntityId(5)));
    assert_eq!(index.remove(&EntityId(7)), Some(3));
    assert_eq!(index.remove(&EntityId(7)), None);
    assert_eq!(index.get(&EntityId(7)), None);
}

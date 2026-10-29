use super::*;

#[test]
fn gatherer_membership_matches_jobs_through_sparse_ids_and_shared_words() {
    let ids = [
        EntityId(0),
        EntityId(1),
        EntityId(63),
        EntityId(64),
        EntityId(u32::MAX),
    ];
    let mut jobs = Gatherers::default();
    let task = || GatherTask::new(7, TileCoord::new(2, 3), ResourceKind::Wood, None);
    let check = |jobs: &Gatherers| {
        assert!(jobs.members.len() <= jobs.tasks.len());
        assert!(jobs.members.values().all(|&word| word != 0));
        for id in ids.into_iter().chain([EntityId(62), EntityId(65)]) {
            assert_eq!(jobs.contains_key(&id), jobs.get(&id).is_some());
        }
    };
    for id in ids {
        jobs.insert(id, task());
        check(&jobs);
    }
    jobs.insert(ids[0], task());
    assert_eq!(jobs.count_target(WorkTarget::Node(7)), 5);
    jobs.insert(
        ids[0],
        GatherTask::new(
            7,
            TileCoord::new(2, 3),
            ResourceKind::Food,
            Some(EntityId(7)),
        ),
    );
    assert_eq!(jobs.count_target(WorkTarget::Node(7)), 4);
    assert_eq!(jobs.count_target(WorkTarget::Farm(EntityId(7))), 1);
    check(&jobs);
    for id in [ids[0], ids[2], ids[4], ids[1], ids[3]] {
        jobs.remove(&id);
        check(&jobs);
    }
    jobs.insert(ids[4], task());
    jobs.clear();
    check(&jobs);
}

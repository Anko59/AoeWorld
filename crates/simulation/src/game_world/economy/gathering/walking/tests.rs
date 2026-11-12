use super::*;

#[test]
fn a_new_contact_subscriber_receives_an_already_notified_tile() {
    let mut gatherers = Gatherers::default();
    let tile = TileCoord::new(20, 20);
    let walk = Walk {
        until: u64::MAX,
        near: Some(tile),
    };
    let first = EntityId(1);
    let second = EntityId(2);
    gatherers.dozes.walking.update(first, walk);
    gatherers.notify_walkers_at(tile);
    gatherers.dozes.walking.update(second, walk);
    gatherers.notify_walkers_at(tile);
    assert_eq!(
        gatherers.dozes.walking.notified,
        BTreeSet::from([first, second])
    );
}

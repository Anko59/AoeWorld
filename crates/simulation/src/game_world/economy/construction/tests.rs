//! Count the completion membership kernel, not shared-host wall time. This
//! synthetic job set is not a full 100-player movement/performance qualification.
use super::*;
use aoe_core::{Seed, WorldConfig};
use std::collections::BTreeMap;

#[test]
fn completion_membership_visits_twenty_thousand_builders_once() {
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(1)).unwrap()).unwrap();
    let mut completed = BTreeMap::new();
    for player in 0..100_u16 {
        for slot in 0..200_u32 {
            let builder = EntityId(u32::from(player) * 200 + slot);
            let building = EntityId(20_000 + builder.0);
            world.economy.builders.insert(
                builder,
                BuildTask {
                    building,
                    attempts: 0,
                    queue: Vec::new(),
                },
            );
            completed.insert(
                building,
                (
                    PlayerId(player),
                    BuildingKind::Farm,
                    TileRect::from_xywh(100, 100, 3, 3),
                ),
            );
        }
    }
    // No live actors are needed to exercise membership lookup and release.
    continuation::COMPLETION_VISITS.with(|visits| visits.set(0));
    let mut changed = Vec::new();
    world.finish_builders(&completed, &mut changed);
    assert_eq!(changed.len(), 20_000);
    assert!(world.economy.builders.is_empty());
    continuation::COMPLETION_VISITS.with(|visits| assert_eq!(visits.get(), 20_000));
}

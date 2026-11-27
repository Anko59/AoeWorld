use super::*;

#[test]
fn members_keep_their_targets_while_the_carrier_waits_for_a_route() {
    let mut w = GameWorld::new(WorldConfig::new(256, 256, aoe_core::Seed(1)).unwrap()).unwrap();
    let at = |x, y| WorldPosition::from_tile_center(aoe_core::TileCoord::new(x, y)).unwrap();
    let ids = (0..6)
        .map(|n| {
            w.spawn_militia(PlayerId(0), at(100 + n % 3, 100 + n / 3))
                .unwrap()
        })
        .collect::<Vec<_>>();
    w.issue_formation_move(
        PlayerId(0),
        &ids,
        Formation::Line,
        at(160, 100),
        false,
        true,
    )
    .unwrap();
    let leader = w.units[w.lookup[&ids[0]]].formation.unwrap();
    let targets = |w: &mut GameWorld, waiting: bool, shift: i32| {
        w.units[w.lookup[&leader]].state.planning = waiting;
        let mut group = w.formations.remove(&leader).unwrap();
        group.anchor.x += shift;
        w.prepare_group_targets(leader, &mut group);
        let targets = group.members.iter().map(|m| m.target).collect::<Vec<_>>();
        w.formations.insert(leader, group);
        targets
    };
    let before = targets(&mut w, false, 0);
    // Slots recomputed around a drifting anchor: a waiting group keeps its
    // targets, so members stand instead of stepping back and forth.
    assert_eq!(targets(&mut w, true, 1024), before);
    let marching = targets(&mut w, false, 0);
    assert_ne!(marching, before);
}

#[test]
fn route_failure_issuer_moves_with_a_formation_carrier() {
    let mut world = GameWorld::new(WorldConfig::new(256, 256, aoe_core::Seed(1)).unwrap()).unwrap();
    let player = PlayerId(0);
    let ids = [20, 22].map(|x| {
        world
            .spawn_kind(
                player,
                UnitKind::Militia,
                WorldPosition::new(x * 1024, 20 * 1024),
            )
            .unwrap()
    });
    world.with_order_issuer(crate::OrderIssuer::Player, |world| {
        world
            .issue_formation_move(
                player,
                &ids,
                Formation::Line,
                WorldPosition::new(120 * 1024, 20 * 1024),
                false,
                false,
            )
            .unwrap();
    });
    let carrier = world.units[world.lookup[&ids[0]]].formation.unwrap();
    let replacement = ids.into_iter().find(|&id| id != carrier).unwrap();
    world.hand_over_route(carrier, replacement);
    assert_eq!(
        world.units[world.lookup[&replacement]].route_issuer,
        Some(crate::OrderIssuer::Player)
    );
}

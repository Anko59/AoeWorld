use super::*;

#[test]
fn members_near_their_slots_hold_while_the_carrier_waits_for_a_route() {
    let mut w = GameWorld::new(WorldConfig::new(256, 256, aoe_core::Seed(1)).unwrap()).unwrap();
    let at = |x, y| WorldPosition::from_tile_center(aoe_core::TileCoord::new(x, y)).unwrap();
    let mut ids = (0..6)
        .map(|n| {
            w.spawn_militia(PlayerId(0), at(100 + n % 3, 100 + n / 3))
                .unwrap()
        })
        .collect::<Vec<_>>();
    let straggler = w.spawn_militia(PlayerId(0), at(103, 100)).unwrap();
    ids.push(straggler);
    w.issue_formation_move(
        PlayerId(0),
        &ids,
        Formation::Line,
        at(160, 100),
        false,
        true,
    )
    .unwrap();
    // Pushed twenty tiles off after the order: its slot stays with the block.
    let index = w.lookup[&straggler];
    w.units[index].state.position = at(103, 120);
    let leader = w.units[w.lookup[&ids[0]]].formation.unwrap();
    let targets = |w: &mut GameWorld, waiting: bool| {
        w.units[w.lookup[&leader]].state.planning = waiting;
        let mut group = w.formations.remove(&leader).unwrap();
        w.prepare_group_targets(leader, &mut group);
        let targets = group
            .members
            .iter()
            .map(|m| (m.id, m.target))
            .collect::<Vec<_>>();
        w.formations.insert(leader, group);
        targets
    };
    let waiting = targets(&mut w, true);
    let marching = targets(&mut w, false);
    let position = |id| w.unit(id).unwrap().position;
    let holds = |targets: &[(EntityId, WorldPosition)]| {
        targets
            .iter()
            .filter(|&&(id, target)| id != straggler && target == position(id))
            .map(|&(id, _)| id)
            .collect::<Vec<_>>()
    };
    // Waiting, members near their slots stand instead of stepping around
    // slots recomputed every tick; one pushed twenty tiles off still closes in.
    let held = holds(&waiting);
    assert!(
        held.iter().any(|id| !holds(&marching).contains(id)),
        "marching {marching:?} waiting {waiting:?}"
    );
    let far = waiting.iter().find(|m| m.0 == straggler).unwrap().1;
    let marched = marching.iter().find(|m| m.0 == straggler).unwrap().1;
    assert_ne!(
        far,
        position(straggler),
        "a far member still closes in: marching target {marched:?}, at {:?}",
        position(straggler)
    );
}

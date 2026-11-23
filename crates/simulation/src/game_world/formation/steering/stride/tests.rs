use super::*;

/// A lone walker and a three-man formation of player 1 marching east, with
/// player 0 as the only human seat when `watched`.
fn march(watched: bool) -> (GameWorld, [EntityId; 2]) {
    let mut w = GameWorld::new(WorldConfig::new(256, 256, aoe_core::Seed(1)).unwrap()).unwrap();
    if watched {
        w.set_human_seats(&[PlayerId(0)]);
    }
    let tile = |x, y| WorldPosition::new(x * 1024, y * 1024);
    let walker = w
        .spawn_kind(PlayerId(1), UnitKind::Militia, tile(60, 60))
        .unwrap();
    w.issue_move(walker, tile(160, 60)).unwrap();
    let army: Vec<_> = (0..3)
        .map(|n| {
            w.spawn_kind(PlayerId(1), UnitKind::Militia, tile(60, 100 + n))
                .unwrap()
        })
        .collect();
    w.issue_formation_move(
        PlayerId(1),
        &army,
        Formation::Line,
        tile(160, 101),
        false,
        false,
    )
    .unwrap();
    (w, [walker, army[1]])
}

#[test]
fn unseen_movers_step_every_fourth_tick_and_cover_the_same_ground() {
    let (mut every, ids) = march(false);
    let (mut strided, _) = march(true);
    let mut steps = [0u32; 2];
    for _ in 0..160 {
        let before = ids.map(|id| strided.unit(id).unwrap().position);
        every.advance();
        strided.advance();
        for (n, id) in ids.iter().enumerate() {
            steps[n] += u32::from(strided.unit(*id).unwrap().position != before[n]);
        }
    }
    for (n, id) in ids.iter().enumerate() {
        assert!(
            (36..=44).contains(&steps[n]),
            "unit {n} stepped on {} of 160 ticks",
            steps[n]
        );
        let x = |w: &GameWorld| w.unit(*id).unwrap().position.x - 60 * 1024;
        let (full, banked) = (x(&every), x(&strided));
        assert!(full > 5 * 1024, "unit {n} barely moved: {full}");
        assert!(
            banked.abs_diff(full) * 10 <= full.unsigned_abs(),
            "{banked} vs {full}"
        );
    }
}

#[test]
fn a_mover_on_a_screen_at_unit_zoom_steps_every_tick() {
    let (mut w, [walker, _]) = march(true);
    w.set_human_views(&[crate::HumanView {
        player: PlayerId(0),
        region: aoe_core::TileRect::from_xywh(50, 40, 120, 40),
        unit_zoom: true,
    }]);
    let mut steps = 0;
    for _ in 0..40 {
        let before = w.unit(walker).unwrap().position;
        w.advance();
        steps += u32::from(w.unit(walker).unwrap().position != before);
    }
    assert!(steps >= 36, "stepped on {steps} of 40 ticks");
}

#[test]
fn an_unseen_group_waiting_for_its_route_steps_every_sixteenth_tick_and_banks_nothing() {
    let (mut w, [_, member]) = march(true);
    let leader = w.units[w.lookup[&member]].formation.unwrap();
    assert_eq!(
        w.stride_of(leader),
        STRIDE,
        "a routed group keeps the stride"
    );
    let index = w.lookup[&leader];
    w.clear_planner(index);
    w.units[index].route.clear();
    w.units[index].state.planning = true;
    assert!(w.waiting_for_route(index));
    assert_eq!(w.stride_of(leader), WAITING_STRIDE);
    let before: Vec<u64> = w.formations[&leader]
        .members
        .iter()
        .map(|m| m.carry)
        .collect();
    w.bank_group_rest(leader);
    let after: Vec<u64> = w.formations[&leader]
        .members
        .iter()
        .map(|m| m.carry)
        .collect();
    assert_eq!(before, after, "a still anchor banks no slot walk");
}

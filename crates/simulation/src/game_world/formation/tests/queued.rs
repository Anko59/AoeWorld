use super::*;

#[test]
fn queued_turn_keeps_a_rear_body_before_the_old_wall_bend() {
    let mut w = GameWorld::new(WorldConfig::new(256, 256, aoe_core::Seed(1)).unwrap()).unwrap();
    let at = |x, y| WorldPosition::from_tile_center(aoe_core::TileCoord::new(x, y)).unwrap();
    let ids = (0..3)
        .map(|n| w.spawn_militia(PlayerId(0), at(100, 105 + n)).unwrap())
        .collect::<Vec<_>>();
    for y in 100..110 {
        w.economy
            .blocked_tiles
            .insert(aoe_core::TileCoord::new(105, y));
    }
    w.collision_closed(aoe_core::TileRect::from_xywh(105, 100, 1, 10));
    let leader = ids[0];
    w.issue_formation_move(
        PlayerId(0),
        &ids,
        Formation::Line,
        at(110, 110),
        false,
        true,
    )
    .unwrap();
    w.issue_formation_move(PlayerId(0), &ids, Formation::Line, at(120, 100), true, true)
        .unwrap();
    let mut group = w.formations.remove(&leader).unwrap();
    // The anchor passed the wall; the rear still approaches its lower end.
    group.anchor = at(110, 110);
    group.path = path::Path::new(
        [at(100, 100), at(100, 110), at(110, 110)],
        w.group_work.clone(),
    );
    group.progress = 20 * 1024;
    group.path_cursor = 1;
    for (n, member) in group.members.iter_mut().enumerate() {
        member.progress = (5 + n as i64) * 1024;
        member.cursor = 0;
        member.following_corridor = true;
    }
    let index = w.lookup[&leader];
    w.units[index].order = None;
    w.units[index].route.clear();
    w.next_formation_order(leader, &mut group);
    w.formations.insert(leader, group);
    w.advance_formations(&mut Vec::new(), &mut Default::default());
    let motion = w.formation_motion(index).unwrap();
    assert!(
        (motion.target.x - at(100, 105).x).abs() <= 512,
        "{motion:?}"
    );
    assert!(
        motion.target.y > at(100, 105).y,
        "follow the old corridor before turning: {motion:?}"
    );
}

#[test]
fn a_paced_front_member_cannot_spend_catch_up_credit_to_race_the_anchor() {
    let mut w = GameWorld::new(WorldConfig::new(256, 256, aoe_core::Seed(1)).unwrap()).unwrap();
    let start = WorldPosition::new(100 * 1024 + 512, 100 * 1024 + 512);
    let id = w.spawn_militia(PlayerId(0), start).unwrap();
    w.issue_formation_move(
        PlayerId(0),
        &[id],
        Formation::Line,
        WorldPosition::new(180 * 1024 + 512, start.y),
        false,
        true,
    )
    .unwrap();
    w.issue_formation_pace(PlayerId(0), id, Some((15, 1)))
        .unwrap();
    for _ in 0..10 {
        w.advance();
    }
    let index = w.lookup[&id];
    let old = w.unit(id).unwrap().position;
    let ahead = WorldPosition::new(old.x + 4096, old.y);
    w.collisions.relocate(id, old.tile_floor(), ahead, false);
    w.units[index].state.position = ahead;
    let group = w.formations.get_mut(&id).unwrap();
    group.members[0].progress = group.progress + 4096;
    group.members[0].following_corridor = true;
    let motion = w.formation_motion(index).unwrap();
    assert!(
        motion.budget <= 15,
        "an ahead body respects the ordinary pace: {motion:?}"
    );
}

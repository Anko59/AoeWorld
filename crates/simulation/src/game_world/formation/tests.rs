use super::*;

#[test]
fn formation_certificate_bound_respects_homogeneous_physical_speed_and_carry() {
    let mut w = GameWorld::new(WorldConfig::new(256, 256, aoe_core::Seed(1)).unwrap()).unwrap();
    let at = WorldPosition::new(100 * 1024, 100 * 1024);
    let id = w.spawn_kind(PlayerId(0), UnitKind::Militia, at).unwrap();
    w.issue_formation_move(
        PlayerId(0),
        &[id],
        Formation::Line,
        WorldPosition::new(125 * 1024, at.y),
        false,
        false,
    )
    .unwrap();
    w.formations.get_mut(&id).unwrap().members[0].carry = (1_u64 << 32) - 1;
    let motion = w.formation_motion(w.lookup[&id]).unwrap();
    let (speed, denominator) = w.movement_speed(UnitKind::Militia, PlayerId(0));
    assert_eq!(motion.budget, speed.div_ceil(denominator) as i32);
    assert_eq!(w.formation_step_bound(), motion.budget + 2);
}

#[test]
fn cohesion_brake_follows_the_rear_tenths_lag_and_tightens_on_arrival() {
    let mut w = GameWorld::new(WorldConfig::new(256, 256, aoe_core::Seed(1)).unwrap()).unwrap();
    let at = WorldPosition::new(100 * 1024, 100 * 1024);
    let ids = (0..2)
        .map(|n| {
            w.spawn_kind(
                PlayerId(0),
                UnitKind::Militia,
                WorldPosition::new(at.x, at.y + n * 2048),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    w.issue_formation_move(
        PlayerId(0),
        &ids,
        Formation::Line,
        WorldPosition::new(150 * 1024, 100 * 1024),
        false,
        false,
    )
    .unwrap();
    let mut group = w.formations.remove(&ids[0]).unwrap();
    group.form_distance = 0;
    // Corridor lag in tiles behind each slot; 64ths of pace braked. On the
    // march the rearmost tenth (here the larger lag) brakes from two tiles,
    // fully from ten.
    group.progress = 20 * 1024;
    for (lags, brake) in [
        ([-3, 0], 0),
        ([2, 2], 0),
        ([4, 0], 2),
        ([3, 6], 4),
        ([7, 1], 5),
        ([9, 30], 8),
    ] {
        for (member, lag) in group.members.iter_mut().zip(lags) {
            member.progress = group.progress + i64::from(member.offset.y) - lag * 1024;
        }
        assert_eq!(w.formation_brake(&group), brake, "lags={lags:?}");
    }
    // Within a block length of the destination, a body's remaining distance
    // to its slot counts as lag too: these bodies never left the start.
    group.progress = group.path.end() - 2 * 1024;
    for member in &mut group.members {
        member.progress = group.progress + i64::from(member.offset.y);
    }
    assert_eq!(w.formation_brake(&group), 8);
    group.attack_move = true;
    assert_eq!(w.formation_brake(&group), 0);
}

impl GameWorld {
    /// Test-only observation of the shared anchor; no simulation instrumentation.
    pub(crate) fn group_march_probe(&self, id: EntityId) -> (i64, i64, (u64, u64), (u64, u64)) {
        let leader = self.units[self.lookup[&id]].formation.unwrap();
        let group = &self.formations[&leader];
        (group.progress, group.form_distance, group.speed, group.pace)
    }
}

#[test]
fn rotation_preserves_cardinal_and_diagonal_integer_rounding() {
    let anchor = WorldPosition::new(1000, 2000);
    let offset = WorldPosition::new(37, -71);
    for (heading, expected) in [
        ((0, 0), (1000, 2000)),
        ((1024, 0), (929, 2037)),
        ((-1024, 0), (1071, 1963)),
        ((0, 1024), (963, 1929)),
        ((0, -1024), (1037, 2071)),
        ((3, 4), (928, 1966)),
        ((-3, -4), (1072, 2034)),
    ] {
        let heading = WorldPosition::new(heading.0, heading.1);
        let expected = WorldPosition::new(expected.0, expected.1);
        assert_eq!(layout::oriented(anchor, offset, heading), expected);
        assert_eq!(
            layout::Rotation::new(heading).oriented(anchor, offset),
            expected
        );
    }
}

#[test]
fn axis_paths_and_adjusted_distance_spans_keep_exact_projection() {
    for end in [(1024, 0), (-1024, 0), (0, 1024), (0, -1024)] {
        let mut path = path::Path::new(
            [WorldPosition::new(0, 0), WorldPosition::new(end.0, end.1)],
            Default::default(),
        );
        let mut cursor = 0;
        assert_eq!(
            path.point(512, &mut cursor).0,
            WorldPosition::new(end.0 / 2, end.1 / 2)
        );
        assert_eq!(
            path.project(
                WorldPosition::new(end.0 * 3 / 4, end.1 * 3 / 4),
                &mut cursor
            ),
            768
        );
        path.lengths[1] = 1025;
        assert_eq!(
            path.point(512, &mut cursor).0,
            WorldPosition::new(end.0.signum() * 511, end.1.signum() * 511)
        );
        assert_eq!(
            path.project(WorldPosition::new(end.0 / 2, end.1 / 2), &mut cursor),
            511
        );
    }
}

#[test]
fn movement_speed_projection_preserves_custom_base_and_age_effects() {
    use crate::game_world::combat::military::apply_movement_speed;
    assert_eq!(
        apply_movement_speed(UnitKind::Scout, Some(111), false),
        Some(111)
    );
    assert_eq!(
        apply_movement_speed(UnitKind::Scout, Some(111), true),
        Some(155)
    );
    assert_eq!(
        apply_movement_speed(UnitKind::Militia, Some(111), true),
        Some(111)
    );
    assert_eq!(apply_movement_speed(UnitKind::Monk, None, true), None);
}

#[test]
fn a_detached_member_rejoins_with_only_one_physical_step_per_tick() {
    let mut w = GameWorld::new(WorldConfig::new(256, 256, aoe_core::Seed(1)).unwrap()).unwrap();
    let at = WorldPosition::new(100 * 1024, 100 * 1024);
    let leader = w.spawn_kind(PlayerId(0), UnitKind::Militia, at).unwrap();
    let from = WorldPosition::new(at.x - 1024, at.y);
    let follower = w.spawn_kind(PlayerId(0), UnitKind::Militia, from).unwrap();
    let goal = WorldPosition::new(150 * 1024, at.y);
    w.issue_formation_move(
        PlayerId(0),
        &[leader, follower],
        Formation::Line,
        goal,
        false,
        false,
    )
    .unwrap();
    let index = w.lookup[&follower];
    let member = &mut w.formations.get_mut(&leader).unwrap().members[1];
    member.detached = true;
    member.ideal = at;
    let recovery = WorldPosition::new(from.x + 256, from.y);
    w.install_formation_route(
        index,
        from,
        recovery,
        recovery,
        VecDeque::new(),
        None,
        false,
        true,
    );
    w.advance_movement();
    assert!(
        !w.is_detached_member(index),
        "the member must actually rejoin"
    );
    let moved = path::distance(from, w.unit(follower).unwrap().position);
    let (speed, denominator) = w.movement_speed(UnitKind::Militia, PlayerId(0));
    assert!(moved > 0, "the recovery must keep walking");
    assert!(
        moved <= speed.div_ceil(denominator) as i64 + 2,
        "double movement: {moved}"
    );
}

#[test]
fn a_reserved_recovery_endpoint_cannot_release_a_formation_member() {
    let mut w = GameWorld::new(WorldConfig::new(256, 256, aoe_core::Seed(1)).unwrap()).unwrap();
    let at = WorldPosition::new(100 * 1024, 100 * 1024);
    let leader = w.spawn_kind(PlayerId(0), UnitKind::Militia, at).unwrap();
    let follower = w
        .spawn_kind(
            PlayerId(0),
            UnitKind::Militia,
            WorldPosition::new(at.x - 1024, at.y),
        )
        .unwrap();
    w.issue_formation_move(
        PlayerId(0),
        &[leader, follower],
        Formation::Line,
        WorldPosition::new(150 * 1024, at.y),
        false,
        false,
    )
    .unwrap();
    let index = w.lookup[&follower];
    let recovery = WorldPosition::new(at.x + 3 * 1024, at.y);
    let blocker = w
        .spawn_kind(
            PlayerId(0),
            UnitKind::Villager,
            WorldPosition::new(at.x + 10 * 1024, at.y),
        )
        .unwrap();
    w.issue_move(blocker, recovery).unwrap();
    let recovery = w.movement_order(blocker).unwrap().destination;
    w.formations.get_mut(&leader).unwrap().members[1].detached = true;
    let from = w.unit(follower).unwrap().position;
    w.install_formation_route(
        index,
        from,
        recovery,
        recovery,
        VecDeque::new(),
        None,
        false,
        true,
    );
    w.tick.0 = (8 - u64::from(follower.0) % 8) % 8;
    assert!(w.destination_point_reserved(recovery, follower));
    let slot = w.formation_destination_slot(follower);
    w.replace_occupied_arrival(index);
    assert_eq!(w.units[index].formation, Some(leader));
    assert_eq!(w.formation_destination_slot(follower), slot);
    assert_eq!(w.units[index].order.unwrap().destination, recovery);
}

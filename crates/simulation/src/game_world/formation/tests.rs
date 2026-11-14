use super::*;

#[test]
fn a_detached_member_rejoining_without_displacement_wakes_acquisition() {
    let build = |sleep| {
        let mut w = GameWorld::new(WorldConfig::new(256, 256, aoe_core::Seed(1)).unwrap()).unwrap();
        w.set_work_sleep(sleep);
        let at = WorldPosition::new(100 * 1024, 100 * 1024);
        let id = w.spawn_kind(PlayerId(0), UnitKind::Militia, at).unwrap();
        w.issue_formation_move(
            PlayerId(0),
            &[id],
            Formation::Line,
            WorldPosition::new(125 * 1024, at.y),
            false,
            true,
        )
        .unwrap();
        // A detached member finished an intermediate route and is idle until
        // the normal rejoin pass restores the remaining group march.
        w.formations.get_mut(&id).unwrap().members[0].detached = true;
        let index = w.lookup[&id];
        w.units[index].order = None;
        w.units[index].state.moving = false;
        w.units[index].state.planning = false;
        w.units[index].state.task = UnitTask::Idle;
        w.active_movers.remove(id);
        w.advance_combat(&mut Vec::new());
        (w, id)
    };
    let (mut on, id) = build(true);
    let (mut off, _) = build(false);
    assert_eq!(on.canonical_hash(), off.canonical_hash());
    let position = on.unit(id).unwrap().position;
    for w in [&mut on, &mut off] {
        w.rejoin_detached_members();
        assert_eq!(w.unit(id).unwrap().position, position);
        assert!(w.unit(id).unwrap().moving);
        w.advance_combat(&mut Vec::new());
    }
    assert_eq!(on.canonical_hash(), off.canonical_hash());
}

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
    pub(crate) fn group_density_probe(&self, id: EntityId) -> String {
        let Some(leader) = self.units[self.lookup[&id]].formation else {
            return "not admitted".to_owned();
        };
        let group = &self.formations[&leader];
        let member = &group.members[group.member_index(id).unwrap()];
        format!(
            "files={} anchor={:?} progress={} width={} member={member:?}",
            group.packing_files, group.anchor, group.progress, group.half_width
        )
    }
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

#[test]
fn road_formation_certificate_covers_the_faster_physical_step() {
    let mut w = GameWorld::new(WorldConfig::new(256, 256, aoe_core::Seed(1)).unwrap()).unwrap();
    let at = WorldPosition::new(100 * 1024 + 512, 100 * 1024 + 512);
    w.insert_building(
        PlayerId(0),
        crate::BuildingKind::Road,
        aoe_core::TileRect::from_xywh(100, 100, 1, 1),
        true,
    )
    .unwrap();
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
    assert!(motion.budget > speed.div_ceil(denominator) as i32);
    assert!(w.formation_step_bound() >= motion.budget + 2);
}

/// A queued destination is a new leg: a member that spent its recovery
/// attempts on the previous one gets its full allowance again.
#[test]
fn a_queued_destination_gives_every_member_its_own_recovery_attempts() {
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
    let mut group = w.formations.remove(&leader).unwrap();
    group.members[1].retries = detach::MAX_RETRIES;
    group
        .queue
        .push_back((WorldPosition::new(150 * 1024, 140 * 1024), false));
    w.next_formation_order(leader, &mut group);
    assert_eq!(group.members[1].retries, 0, "the new leg starts fresh");
    w.formations.insert(leader, group);
}

/// Measured on the river army: a detour ended in the tick the rest of the group
/// finished, the group settled, and nothing ever rejoined the soldier.
#[test]
fn a_group_does_not_settle_around_a_member_whose_detour_just_ended() {
    let mut w = GameWorld::new(WorldConfig::new(256, 256, aoe_core::Seed(1)).unwrap()).unwrap();
    let at = WorldPosition::new(100 * 1024, 100 * 1024);
    let leader = w.spawn_kind(PlayerId(0), UnitKind::Militia, at).unwrap();
    let follower = w
        .spawn_kind(
            PlayerId(0),
            UnitKind::Militia,
            WorldPosition::new(at.x, at.y + 2048),
        )
        .unwrap();
    w.issue_formation_move(
        PlayerId(0),
        &[leader, follower],
        Formation::Line,
        WorldPosition::new(110 * 1024, at.y),
        false,
        false,
    )
    .unwrap();
    for _ in 0..2_000 {
        w.advance();
        if w.formations[&leader].settled {
            break;
        }
    }
    assert!(w.formations[&leader].settled);
    let slot = w.formation_destination_slot(follower).unwrap();
    // A detour towards a corridor point, whose end is not the member's slot.
    let index = w.lookup[&follower];
    let moved = WorldPosition::new(slot.x - 6 * 1024, slot.y);
    let old = w.units[index].state.position;
    w.collisions
        .relocate(follower, old.tile_floor(), moved, false);
    w.units[index].state.position = moved;
    let group = w.formations.get_mut(&leader).unwrap();
    group.settled = false;
    group.members[1].arrived = false;
    w.detach_member(follower);
    let slot = WorldPosition::new(slot.x + 6 * 1024, slot.y);
    w.formations.get_mut(&leader).unwrap().members[1].goal = slot;
    for _ in 0..2_000 {
        w.advance();
    }
    assert_eq!(w.units[index].state.position, slot);
    assert!(!w.formations[&leader].members[1].detached);
}

impl GameWorld {
    /// (files, form_start, progress, path end) of the group holding `id`.
    pub(crate) fn group_files_probe(&self, id: EntityId) -> (usize, i64, i64, i64) {
        let group = &self.formations[&self.units[self.lookup[&id]].formation.unwrap()];
        (
            group.packing_files,
            group.form_start,
            group.progress,
            group.path.end(),
        )
    }
}

#[test]
fn an_ahead_member_keeps_advancing_on_an_offensive_click() {
    for attack_move in [false, true] {
        let mut w = GameWorld::new(WorldConfig::new(256, 256, aoe_core::Seed(1)).unwrap()).unwrap();
        let start = WorldPosition::new(100 * 1024 + 512, 100 * 1024 + 512);
        let id = w.spawn_kind(PlayerId(0), UnitKind::Militia, start).unwrap();
        w.issue_formation_move(
            PlayerId(0),
            &[id],
            Formation::Line,
            WorldPosition::new(150 * 1024 + 512, start.y),
            false,
            attack_move,
        )
        .unwrap();
        // Traffic can leave a body ahead of its group's forming slot. The
        // offensive posture must not make it stand waiting for that slot.
        for _ in 0..10 {
            w.advance();
        }
        let old = w.unit(id).unwrap().position;
        let ahead = WorldPosition::new(old.x + 4 * 1024, old.y);
        let index = w.lookup[&id];
        w.collisions.relocate(id, old.tile_floor(), ahead, false);
        w.units[index].state.position = ahead;
        let group = w.formations.get_mut(&id).unwrap();
        group.direct = false;
        group.form_distance = 0;
        group.members[0].following_corridor = false;
        group.members[0].progress = group.progress + 4 * 1024;
        w.advance_formations(&mut Vec::new(), &mut Default::default());
        let motion = w.formation_motion(index).unwrap();
        assert!(
            motion.target.x > ahead.x,
            "attack_move={attack_move} {motion:?} {}",
            w.group_density_probe(id)
        );
    }
}

mod queued;

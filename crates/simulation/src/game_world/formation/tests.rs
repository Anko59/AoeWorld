use super::*;

#[test]
fn cohesion_waits_only_for_rear_stragglers() {
    let mut w = GameWorld::new(WorldConfig::new(256, 256, aoe_core::Seed(1)).unwrap()).unwrap();
    let at = WorldPosition::new(100 * 1024, 100 * 1024);
    let id = w.spawn_kind(PlayerId(0), UnitKind::Militia, at).unwrap();
    w.issue_formation_move(
        PlayerId(0),
        &[id],
        Formation::Line,
        WorldPosition::new(125 * 1024, 100 * 1024),
        false,
        false,
    )
    .unwrap();
    let mut group = w.formations.remove(&id).unwrap();
    for (dx, dy, cohesive) in [(0, 10, true), (-10, 0, true), (1, 0, true), (3, 0, false)] {
        group.members[0].ideal = WorldPosition::new(at.x + dx * 1024, at.y + dy * 1024);
        assert_eq!(w.formation_cohesive(&group), cohesive, "delta={dx},{dy}");
    }
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
        let mut path =
            path::Path::new([WorldPosition::new(0, 0), WorldPosition::new(end.0, end.1)]);
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

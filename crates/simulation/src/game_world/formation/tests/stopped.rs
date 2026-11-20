use super::*;

#[test]
fn stopped_and_arrived_formations_clear_waypoints_and_do_no_member_work() {
    for stop in [false, true] {
        let mut w = GameWorld::new(WorldConfig::new(256, 256, aoe_core::Seed(1)).unwrap()).unwrap();
        let ids: Vec<_> = (0..6)
            .map(|i| {
                w.spawn_kind(
                    PlayerId(0),
                    UnitKind::Militia,
                    WorldPosition::new(100 * 1024, (100 + i) * 1024),
                )
                .unwrap()
            })
            .collect();
        w.issue_formation_move(
            PlayerId(0),
            &ids,
            Formation::Line,
            WorldPosition::new(112 * 1024, 102 * 1024),
            false,
            false,
        )
        .unwrap();
        assert!(ids.iter().all(|&id| !w.movement_waypoints(id).is_empty()));
        if stop {
            w.issue_stop_selection(PlayerId(0), &ids).unwrap();
        } else {
            for _ in 0..600 {
                w.advance();
            }
        }
        assert!(ids.iter().all(|&id| w.movement_waypoints(id).is_empty()));
        assert!(ids.iter().all(|&id| !w.unit(id).unwrap().moving));
        assert!(ids.iter().all(|&id| !w.active_movers.contains(id)));
        let before = w.route_scheduler_stats().metrics.group_member_work;
        for _ in 0..20 {
            w.advance();
        }
        assert_eq!(
            w.route_scheduler_stats().metrics.group_member_work,
            before,
            "stop={stop}"
        );
    }
}

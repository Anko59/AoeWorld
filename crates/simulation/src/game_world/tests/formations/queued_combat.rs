use super::*;
use aoe_core::{CombatStance, Formation};

fn near_waypoint(position: WorldPosition, waypoint: WorldPosition) -> bool {
    // Arrivals can use a neighbouring tile when another member occupies the target.
    position.x.abs_diff(waypoint.x) <= 1024 && position.y.abs_diff(waypoint.y) <= 1024
}

fn march() -> (
    GameWorld,
    [EntityId; 2],
    EntityId,
    WorldPosition,
    WorldPosition,
) {
    let mut w = wide_world();
    w.set_fog_disabled(true);
    let ids = [center(40, 40), center(40, 41)]
        .map(|at| w.spawn_kind(PlayerId(0), UnitKind::Knight, at).unwrap());
    let enemy = w
        .spawn_kind(PlayerId(1), UnitKind::Militia, center(49, 40))
        .unwrap();
    w.issue_stance(PlayerId(1), &[enemy], CombatStance::StandGround)
        .unwrap();

    let first = center(65, 40);
    let second = center(64, 62);
    let last = center(79, 62);
    for (at, queued, attack_move) in [(first, false, true), (second, true, false)] {
        w.issue_formation_move(PlayerId(0), &ids, Formation::Line, at, queued, attack_move)
            .unwrap();
    }
    for id in ids {
        w.issue_queued_leg(id, last, true).unwrap();
    }
    (w, ids, enemy, second, last)
}

#[test]
fn formations_keep_queued_legs_while_fighting_and_arrive_at_the_last_waypoint() {
    let (mut w, ids, enemy, second, last) = march();
    let initial = w.movement_waypoints(ids[0]);
    assert_eq!(w.movement_waypoints(ids[1])[..2], initial[..2]);
    assert_ne!(initial[0].order_id, 0);
    assert_ne!(initial[0].order_id, initial[1].order_id);
    assert_eq!(initial[2].order_id, 0);
    let mut fought = [false; 2];
    let mut resumed = [false; 2];
    let mut visited = [false; 2];
    for _ in 0..2400 {
        w.advance();
        for (n, id) in ids.into_iter().enumerate() {
            let unit = w.unit(id).unwrap();
            if unit.attack_target == Some(enemy) {
                fought[n] = true;
                let waypoints = w.move_waypoints(id);
                assert_eq!(waypoints.len(), 3, "the route vanished during combat");
                assert!(waypoints[0].1);
                assert_eq!(waypoints[1..], [(second, false), (last, true)]);
            }
            let markers = w.movement_waypoints(id);
            if unit.attack_target == Some(enemy) {
                assert_eq!(
                    markers[..2],
                    initial[..2],
                    "combat changed the shared waypoints"
                );
            }
            if near_waypoint(unit.position, second) {
                visited[n] = true;
            }
            if markers.first() == Some(&initial[1]) {
                resumed[n] = true;
            }
        }
    }
    assert_eq!(fought, [true; 2]);
    assert_eq!(resumed, [true; 2]);
    assert_eq!(visited, [true; 2]);
    assert!(w.unit(enemy).is_none_or(|u| u.hp == 0));
    for id in ids {
        assert!(near_waypoint(w.unit(id).unwrap().position, last));
        assert!(w.move_waypoints(id).is_empty());
    }
}

#[test]
fn stop_clears_a_fighting_members_march() {
    let (mut w, [id, ally], enemy, _, _) = march();
    for _ in 0..240 {
        w.advance();
        if w.unit(id).unwrap().attack_target == Some(enemy) {
            w.issue_stop(PlayerId(0), id).unwrap();
            assert!(w.move_waypoints(id).is_empty());
            assert_eq!(w.unit(id).unwrap().attack_target, None);
            assert!(!w.move_waypoints(ally).is_empty());
            return;
        }
    }
    panic!("the knight did not break off to fight");
}

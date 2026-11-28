use super::*;
use aoe_core::CombatStance;

const OWN: PlayerId = PlayerId(0);
const ENEMY: PlayerId = PlayerId(1);
const OBSTACLES: [BuildingKind; 5] = [
    BuildingKind::PalisadeWall,
    BuildingKind::StoneWall,
    BuildingKind::PalisadeGate,
    BuildingKind::StoneGate,
    BuildingKind::Road,
];

fn obstacle(w: &mut GameWorld, kind: BuildingKind) -> EntityId {
    let height = if kind.is_gate() { 4 } else { 1 };
    w.insert_building(ENEMY, kind, TileRect::from_xywh(102, 100, 1, height), true)
        .unwrap()
}

#[test]
fn walls_gates_and_roads_can_be_attacked_automatically_when_alone() {
    for kind in OBSTACLES {
        let mut w = world();
        let soldier = w.spawn_militia(OWN, at(100, 100)).unwrap();
        let target = obstacle(&mut w, kind);
        let hp = w.building(target).unwrap().hp;
        advance(&mut w, 20);
        assert_eq!(
            w.unit(soldier).unwrap().attack_target,
            Some(target),
            "{kind:?}"
        );
        ticks_until(&mut w, 100, |w| w.building(target).unwrap().hp < hp);
    }
}

#[test]
fn a_farther_house_outranks_each_obstacle_and_explicit_attacks_still_work() {
    for kind in OBSTACLES {
        let mut w = world();
        let soldier = w.spawn_militia(OWN, at(100, 100)).unwrap();
        let target = obstacle(&mut w, kind);
        let house = w
            .insert_building(
                ENEMY,
                BuildingKind::House,
                TileRect::from_xywh(104, 100, 2, 2),
                true,
            )
            .unwrap();
        advance(&mut w, 20);
        assert_eq!(
            w.unit(soldier).unwrap().attack_target,
            Some(house),
            "{kind:?}"
        );
        w.issue_attack(OWN, soldier, target).unwrap();
        w.advance();
        assert_eq!(
            w.unit(soldier).unwrap().attack_target,
            Some(target),
            "{kind:?}"
        );
    }
}

#[test]
fn an_obstacle_attack_yields_to_a_house_that_comes_into_sight() {
    let mut w = world();
    let soldier = w.spawn_militia(OWN, at(100, 100)).unwrap();
    let wall = obstacle(&mut w, BuildingKind::StoneWall);
    advance(&mut w, 20);
    assert_eq!(w.unit(soldier).unwrap().attack_target, Some(wall));
    let house = w
        .insert_building(
            ENEMY,
            BuildingKind::House,
            TileRect::from_xywh(103, 102, 2, 2),
            true,
        )
        .unwrap();
    ticks_until(&mut w, 20, |w| {
        w.unit(soldier).unwrap().attack_target == Some(house)
    });
}

#[test]
fn a_stand_ground_unit_ignores_a_wall_while_it_can_see_an_enemy_past_its_range() {
    let mut w = world();
    let archer = w.spawn_kind(OWN, UnitKind::Archer, at(100, 100)).unwrap();
    w.issue_stance(OWN, &[archer], CombatStance::StandGround)
        .unwrap();
    let wall = obstacle(&mut w, BuildingKind::PalisadeWall);
    let enemy = w.spawn_villager(ENEMY, at(105, 100)).unwrap();
    advance(&mut w, 10);
    assert!(w.unit_visible_to(OWN, enemy));
    assert_eq!(w.unit(archer).unwrap().attack_target, None);
    w.issue_delete(ENEMY, &[enemy]).unwrap();
    ticks_until(&mut w, 20, |w| {
        w.unit(archer).unwrap().attack_target == Some(wall)
    });
}

#[test]
fn attack_move_does_not_leave_the_march_for_an_off_route_wall_ahead_of_a_house() {
    let mut w = world();
    let soldier = w.spawn_militia(OWN, at(100, 100)).unwrap();
    let wall = w
        .insert_building(
            ENEMY,
            BuildingKind::StoneWall,
            TileRect::from_xywh(102, 101, 1, 1),
            true,
        )
        .unwrap();
    let house = w
        .insert_building(
            ENEMY,
            BuildingKind::House,
            TileRect::from_xywh(104, 100, 2, 2),
            true,
        )
        .unwrap();
    w.issue_formation_move(
        OWN,
        &[soldier],
        aoe_core::Formation::Line,
        at(119, 100),
        false,
        true,
    )
    .unwrap();
    ticks_until(&mut w, 40, |w| {
        let target = w.unit(soldier).unwrap().attack_target;
        assert_ne!(target, Some(wall));
        target == Some(house)
    });
}

#[test]
fn attack_move_can_stop_to_hit_an_isolated_gate() {
    // Ranged units always find a free spot, so later obstacle rechecks find
    // the same gate again; that must not restart the attack.
    for kind in [UnitKind::Militia, UnitKind::Archer] {
        let mut w = world();
        let soldier = w.spawn_kind(OWN, kind, at(100, 100)).unwrap();
        let gate = obstacle(&mut w, BuildingKind::PalisadeGate);
        let hp = w.building(gate).unwrap().hp;
        w.issue_formation_move(
            OWN,
            &[soldier],
            aoe_core::Formation::Line,
            at(110, 100),
            false,
            true,
        )
        .unwrap();
        ticks_until(&mut w, 100, |w| w.building(gate).unwrap().hp < hp);
        let resume = w.combat.attackers[&soldier].resume;
        assert!(resume.is_some(), "{kind:?} keeps its march slot");
        for _ in 0..w.config.ticks(20) {
            w.advance();
        }
        assert_eq!(
            w.unit(soldier).unwrap().attack_target,
            Some(gate),
            "{kind:?}"
        );
        assert_eq!(w.combat.attackers[&soldier].resume, resume, "{kind:?}");
    }
}

use super::super::Village;
use aoe_core::PlayerId;
use aoe_protocol::{UnitKind, UnitTask};
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn idle_military_includes_armies_and_warships_but_not_workers_or_transports() {
    for kind in UnitKind::ALL {
        let mut village = Village::default();
        let mut unit = super::workers::villager(UnitTask::Idle);
        unit.kind = kind;
        village.remember_idle(PlayerId(0), &unit);
        let military = !matches!(
            kind,
            UnitKind::Villager | UnitKind::FishingShip | UnitKind::TransportShip
        );
        assert_eq!(
            village.idle_military.len(),
            usize::from(military),
            "{kind:?}"
        );
        assert_eq!(
            village.idle_villagers.len(),
            usize::from(kind == UnitKind::Villager)
        );
        unit.player = PlayerId(1);
        village.remember_idle(PlayerId(0), &unit);
        assert_eq!(village.idle_military.len(), 0, "Foreign {kind:?}");
        assert_eq!(village.idle_villagers.len(), 0);
    }
}

#[wasm_bindgen_test]
fn dead_planning_attacking_and_auto_scouting_soldiers_are_not_idle() {
    let base = super::workers::villager(UnitTask::Idle);
    for scenario in 0..6 {
        let mut village = Village::default();
        let mut unit = base;
        unit.kind = UnitKind::Scout;
        village.remember_idle(PlayerId(0), &unit);
        assert_eq!(village.idle_military.len(), 1);
        match scenario {
            0 => unit.hp = 0,
            1 => unit.planning = true,
            2 => unit.moving = true,
            3 => unit.attack_target = Some(aoe_core::EntityId(9)),
            4 => unit.auto_scout = true,
            _ => unit.task = UnitTask::Attacking,
        }
        village.remember_idle(PlayerId(0), &unit);
        assert_eq!(village.idle_military.len(), 0, "scenario {scenario}");
    }
}

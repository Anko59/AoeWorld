use super::super::construction::strategic::choose;
use super::workers::villager;
use aoe_core::{EntityId, PlayerId, TileCoord, WorldPosition};
use aoe_protocol::{UnitKind, UnitTask};
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn far_build_prefers_free_idle_then_available_and_uses_managed_workers_when_needed() {
    let mut units = Vec::new();
    for (id, x, task) in [
        (1, 2, UnitTask::Gathering),
        (3, 8, UnitTask::Idle),
        (2, 8, UnitTask::Idle),
        (4, 1, UnitTask::Idle),
    ] {
        let mut u = villager(task);
        u.id = EntityId(id);
        u.position = WorldPosition::from_tile_center(TileCoord::new(x, 10)).unwrap();
        if id == 4 {
            u.managed_by = Some(EntityId(99));
        }
        units.push(u);
    }
    let target = TileCoord::new(0, 10);
    assert_eq!(
        choose(units.iter(), PlayerId(0), target),
        vec![EntityId(2), EntityId(3)]
    );
    units.reverse();
    assert_eq!(
        choose(units.iter(), PlayerId(0), target),
        vec![EntityId(2), EntityId(3)]
    );
    for u in &mut units {
        if u.id.0 == 2 || u.id.0 == 3 {
            u.task = UnitTask::Building;
        }
    }
    assert_eq!(choose(units.iter(), PlayerId(0), target), vec![EntityId(1)]);
    for u in &mut units {
        if u.id.0 == 1 {
            u.player = PlayerId(1);
        }
    }
    assert_eq!(choose(units.iter(), PlayerId(0), target), vec![EntityId(4)]);
    for u in &mut units {
        u.hp = 0;
    }
    assert!(choose(units.iter(), PlayerId(0), target).is_empty());
    for u in &mut units {
        u.hp = 25;
        u.kind = UnitKind::Militia;
    }
    assert!(choose(units.iter(), PlayerId(0), target).is_empty());
}

#[wasm_bindgen_test]
fn far_build_admission_honours_twenty_burst_and_ten_per_second_and_pending_ack() {
    use super::super::construction::strategic::Admission;
    let mut budget = Admission::default();
    for _ in 0..20 {
        assert!(budget.ready(0.0));
        budget.sent(0.0);
    }
    assert!(!budget.ready(0.0));
    assert!(!budget.ready(99.0));
    assert!(budget.ready(100.0));
    budget.sent(100.0);
    budget.pending = Some(1);
    assert!(!budget.ready(200.0));
    budget.pending = None;
    assert!(budget.ready(200.0));
}

#[wasm_bindgen_test]
fn far_build_never_recruits_across_the_map_or_prefers_distant_idle_workers() {
    let mut nearby = villager(UnitTask::Gathering);
    nearby.id = EntityId(1);
    nearby.position = WorldPosition::from_tile_center(TileCoord::new(12, 10)).unwrap();
    let mut distant = villager(UnitTask::Idle);
    distant.id = EntityId(2);
    distant.position = WorldPosition::from_tile_center(TileCoord::new(100, 10)).unwrap();
    assert_eq!(
        choose(
            [&distant, &nearby].into_iter(),
            PlayerId(0),
            TileCoord::new(10, 10)
        ),
        vec![EntityId(1)]
    );
    assert!(choose([&distant].into_iter(), PlayerId(0), TileCoord::new(10, 10)).is_empty());
    nearby.position = WorldPosition::from_tile_center(TileCoord::new(74, 10)).unwrap();
    assert!(choose([&nearby].into_iter(), PlayerId(0), TileCoord::new(10, 10)).is_empty());
    nearby.position = WorldPosition::from_tile_center(TileCoord::new(73, 10)).unwrap();
    assert_eq!(
        choose([&nearby].into_iter(), PlayerId(0), TileCoord::new(10, 10)),
        vec![EntityId(1)]
    );
}

use super::*;
use aoe_core::{PlayerId, WorldPosition};
use wasm_bindgen_test::wasm_bindgen_test;

fn state(x: i32, moving: bool) -> GameplayUnitState {
    GameplayUnitState {
        id: EntityId(1),
        player: PlayerId(1),
        position: WorldPosition::new(x, 0),
        moving,
        planning: false,
        facing: 0,
    }
}

fn moving_history(hz: u32) -> (Presentation, GameplayUnitState) {
    let mut presentation = Presentation::default();
    presentation.reset(hz);
    let mut old = None;
    for tick in 10..=13 {
        let unit = state((tick - 10) as i32 * 100, true);
        assert!(presentation.receive(tick, (tick - 10) as f64 * 1_000.0 / f64::from(hz)));
        presentation.remember(old.as_ref(), &unit, tick);
        old = Some(unit);
    }
    (presentation, old.unwrap())
}

#[wasm_bindgen_test]
fn fractional_frames_advance_at_negotiated_hz_without_extrapolation() {
    for hz in [1, 10, 20, 60] {
        let (mut p, unit) = moving_history(hz);
        let arrival = 3_000.0 / f64::from(hz);
        p.advance(arrival, false);
        assert_eq!(p.position(unit)[0], 100.0 / 1024.0);
        p.advance(arrival + 250.0 / f64::from(hz), false);
        assert_eq!(p.position(unit)[0], 125.0 / 1024.0);
        p.advance(arrival + 500.0 / f64::from(hz), false);
        assert_eq!(p.position(unit)[0], 150.0 / 1024.0);
        p.advance(100_000.0, false);
        assert_eq!(p.position(unit), unit.position.as_tiles());
        assert!(!p.receive(12, 100_010.0));
        let tick = p.tick;
        p.advance(1.0, false);
        p.advance(f64::NAN, false);
        assert_eq!(p.tick, tick);
    }
}

fn remove_missing(
    p: &mut Presentation,
    old: &BTreeMap<EntityId, GameplayUnitState>,
    resident: &BTreeMap<EntityId, GameplayUnitState>,
) {
    for id in old.keys() {
        if !resident.contains_key(id) {
            p.remove(*id);
        }
    }
}

#[wasm_bindgen_test]
fn snapshot_retention_duplicate_ticks_pruning_and_reset_are_bounded() {
    let (mut p, unit) = moving_history(20);
    p.advance(175.0, false);
    let before = p.position(unit);
    assert!(p.receive(13, 175.0));
    p.remember(Some(&unit), &unit, 13);
    let old_units = BTreeMap::from([(unit.id, unit)]);
    remove_missing(&mut p, &old_units, &old_units);
    assert_eq!(p.position(unit), before);
    assert_eq!(p.history[&unit.id].len(), 4);
    for tick in 14..100 {
        p.remember(Some(&unit), &unit, tick);
    }
    assert_eq!(p.history[&unit.id].len(), 8);
    remove_missing(&mut p, &old_units, &BTreeMap::new());
    assert!(p.history.is_empty());
    p.reset(20);
    assert!(p.anchor.is_none());
    assert!(p.receive(0, 0.0));
    assert_eq!(p.tick, 0.0);
}

#[wasm_bindgen_test]
fn canvas_positions_hold_between_bounded_presentations() {
    let (mut p, unit) = moving_history(20);
    p.advance(150.0, true);
    let first = p.position(unit);
    p.advance(166.0, true);
    assert_eq!(p.position(unit), first);
    p.advance(184.0, true);
    assert!(p.position(unit)[0] > first[0]);
}

#[wasm_bindgen_test]
fn stationary_sparse_ticks_stop_and_discontinuity_never_invent_routes() {
    let (mut p, moving) = moving_history(20);
    let stopped = state(400, false);
    p.remember(Some(&moving), &stopped, 14);
    p.receive(14, 200.0);
    p.advance(1000.0, false);
    assert_eq!(p.position(stopped), stopped.position.as_tiles());
    let next = state(410, true);
    p.remember(Some(&stopped), &next, 18);
    assert_eq!(p.history[&next.id][5].tick, 17);
    assert_eq!(p.history[&next.id][5].position, stopped.position);
    let jump = state(50_000, true);
    p.remember(Some(&next), &jump, 19);
    assert_eq!(p.history[&jump.id].len(), 1);
    assert_eq!(p.position(jump), jump.position.as_tiles());
    p.remove(jump.id);
    assert!(p.history.is_empty());
}

#[wasm_bindgen_test]
fn bounded_deque_matches_eight_sample_chronology_without_ninth_slot_growth() {
    let mut window = VecDeque::new();
    let mut reference = std::collections::VecDeque::new();
    for tick in 0..40 {
        let sample = Sample {
            tick,
            position: WorldPosition::new(tick as i32, 0),
        };
        push_sample(&mut window, sample);
        assert!(window.capacity() <= 8);
        reference.push_back(sample);
        while reference.len() > 8 {
            reference.pop_front();
        }
        assert_eq!(window.len(), reference.len());
        for (actual, expected) in window.iter().zip(&reference) {
            assert_eq!(actual.tick, expected.tick);
            assert_eq!(actual.position, expected.position);
        }
    }
    window.pop_back();
    assert_eq!(window.back().unwrap().tick, 38);
    push_sample(
        &mut window,
        Sample {
            tick: 39,
            position: WorldPosition::new(99, 0),
        },
    );
    assert_eq!(window.len(), 8);
    assert_eq!(window.back().unwrap().position, WorldPosition::new(99, 0));
    window.clear();
    window.pop_back();
    assert!(window.is_empty());
    assert!(window.back().is_none());
}

#[wasm_bindgen_test]
fn long_idle_small_step_has_a_recent_anchor_but_large_jump_does_not() {
    let idle = state(0, false);
    let moved = state(100, true);
    let mut p = Presentation::default();
    p.reset(20);
    p.receive(10, 500.0);
    p.remember(None, &idle, 10);
    // Empty changed-unit heartbeats kept the authoritative unit idle.
    p.receive(89, 4450.0);
    p.advance(4450.0, false);
    p.receive(90, 4500.0);
    p.remember(Some(&idle), &moved, 90);
    let history = &p.history[&moved.id];
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].tick, 89);
    assert_eq!(history[0].position, idle.position);
    p.advance(4500.0, false);
    assert_eq!(p.position(moved), idle.position.as_tiles());
    p.advance(4575.0, false);
    assert_eq!(p.position(moved)[0], 50.0 / 1024.0);
    p.advance(4600.0, false);
    assert_eq!(p.position(moved), moved.position.as_tiles());
    p.remember(Some(&moved), &state(90, true), 89);
    assert_eq!(p.history[&moved.id].len(), 2);

    for gap in [5, 80] {
        p.reset(20);
        p.remember(None, &idle, 10);
        let jump = state(50 * 1024, true);
        p.remember(Some(&idle), &jump, 10 + gap);
        assert_eq!(p.history[&jump.id].len(), 1);
        assert_eq!(p.position(jump), jump.position.as_tiles());
    }
    p.reset(20);
    p.remember(Some(&idle), &moved, 0);
    assert_eq!(p.history[&moved.id].len(), 1);
    assert_eq!(p.history[&moved.id].back().unwrap().tick, 0);
}

#[wasm_bindgen_test]
fn deque_history_bytes_and_entity_count_stay_bounded() {
    let sample_bytes = std::mem::size_of::<Sample>();
    let deque_bytes = std::mem::size_of::<VecDeque<Sample>>();
    assert_eq!(sample_bytes, 16);
    assert_eq!(deque_bytes, 4 * std::mem::size_of::<usize>());
    wasm_bindgen_test::console_log!(
        "presentation budget: max8sample heap={} bytes, VecDeque metadata={} bytes, max entities={}, max metadata+samples={} bytes (excludes BTree node/key and allocator overhead)",
        sample_bytes * 8,
        deque_bytes,
        aoe_protocol::MAX_SUBSCRIBED_UNITS,
        (deque_bytes + sample_bytes * 8) * aoe_protocol::MAX_SUBSCRIBED_UNITS,
    );
    let mut p = Presentation::default();
    let mut unit = state(0, false);
    for id in 0..aoe_protocol::MAX_SUBSCRIBED_UNITS {
        unit.id = EntityId(u32::try_from(id).unwrap());
        p.remember(None, &unit, 1);
    }
    assert_eq!(p.history.len(), aoe_protocol::MAX_SUBSCRIBED_UNITS);
    assert!(
        p.history
            .values()
            .all(|window| window.len() <= 8 && window.capacity() <= 8)
    );
    let actual_sample_heap_bytes = p
        .history
        .values()
        .map(|window| window.capacity() * sample_bytes)
        .sum::<usize>();
    assert!(actual_sample_heap_bytes <= sample_bytes * 8 * aoe_protocol::MAX_SUBSCRIBED_UNITS);
    wasm_bindgen_test::console_log!(
        "presentation max-idle sample allocation payload={} bytes (excludes metadata/nodes/allocator)",
        actual_sample_heap_bytes,
    );
    unit.id = EntityId(u32::try_from(aoe_protocol::MAX_SUBSCRIBED_UNITS).unwrap());
    p.remember(None, &unit, 1);
    assert_eq!(p.history.len(), aoe_protocol::MAX_SUBSCRIBED_UNITS);
    assert!(!p.history.contains_key(&unit.id));
    p.remove(EntityId(0));
    p.remember(None, &unit, 1);
    assert_eq!(p.history.len(), aoe_protocol::MAX_SUBSCRIBED_UNITS);
    assert!(p.history.contains_key(&unit.id));
}

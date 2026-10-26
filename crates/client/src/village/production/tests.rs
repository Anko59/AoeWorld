use super::*;
use aoe_core::TileRect;
use aoe_protocol::{Age, BuildingKind, ConstructionState, Stockpile, TrainingState};
use wasm_bindgen_test::wasm_bindgen_test;

fn barracks(id: u32, count: usize) -> GameplayBuildingState {
    GameplayBuildingState {
        id: EntityId(id),
        player: PlayerId(0),
        kind: BuildingKind::Barracks,
        footprint: TileRect::from_xywh(10, 10, 3, 3),
        construction: ConstructionState::Complete,
        progress_permille: 1000,
        hp: 1200,
        max_hp: 1200,
        los: 6,
        food_remaining: None,
        training_queue: vec![
            TrainingState {
                kind: UnitKind::Militia,
                progress_ticks: 0,
                total_ticks: 420
            };
            count
        ],
        research: None,
        last_hit: None,
        age: Age::Dark,
        gate_open: false,
        rally_point: None,
        farm_reseeds: 0,
        garrison: Vec::new(),
    }
}

#[wasm_bindgen_test]
fn shortest_queue_counts_burst_clicks_and_ignores_ineligible_selected_buildings() {
    let mut buildings = vec![
        barracks(1, 2),
        barracks(2, 0),
        barracks(3, 1),
        barracks(4, 0),
        barracks(5, 0),
    ];
    buildings[3].player = PlayerId(1);
    buildings[4].construction = ConstructionState::UnderConstruction;
    let player = GameplayPlayerState {
        player: PlayerId(0),
        stockpile: Stockpile {
            food: 10000,
            gold: 10000,
            ..Default::default()
        },
        age: Age::Dark,
        researched: 0,
        population_used: 5,
        population_cap: 5,
        market_prices: Default::default(),
    };
    let mut selected = Selection::default();
    selected.same_buildings(
        buildings.iter().map(|b| (b.id, b.player, b.kind, true)),
        PlayerId(0),
        BuildingKind::Barracks,
        EntityId(1),
    );
    selected.building_id = Some(EntityId(5));
    assert_eq!(
        representative(&buildings, &selected).unwrap().id,
        EntityId(1)
    );
    let mut pending = PendingTraining::default();
    for (sequence, expected) in [2, 2, 3, 1, 2].into_iter().enumerate() {
        let target = shortest(
            &buildings,
            &selected,
            Some(&player),
            PlayerId(0),
            UnitKind::Militia,
            &pending,
        )
        .unwrap();
        assert_eq!(target.id, EntityId(expected));
        pending.record(sequence as u64, target.id);
    }
    for building in &mut buildings[..3] {
        building.training_queue = barracks(0, 15).training_queue;
    }
    assert!(
        shortest(
            &buildings,
            &selected,
            Some(&player),
            PlayerId(0),
            UnitKind::Militia,
            &pending
        )
        .is_none()
    );
}

#[wasm_bindgen_test]
fn pending_training_is_bounded_and_reconciles_acknowledgements_with_authoritative_ticks() {
    let mut pending = PendingTraining::default();
    pending.record(1, EntityId(2));
    pending.acknowledge(1, CommandResult::Accepted, 10, 9);
    pending.observe(10);
    assert_eq!(pending.count(EntityId(2)), 1);
    pending.observe(11);
    assert_eq!(pending.count(EntityId(2)), 0);
    pending.record(2, EntityId(3));
    pending.acknowledge(2, CommandResult::RejectedTrainingQueueFull, 11, 11);
    assert_eq!(pending.count(EntityId(3)), 0);
    for sequence in 0..100 {
        pending.record(sequence, EntityId(1));
    }
    assert!(!pending.can_send());
    assert_eq!(
        pending.count(EntityId(1)),
        aoe_protocol::MAX_PENDING_COMMANDS_PER_CONNECTION
    );
    let retained = pending.resident_bytes();
    assert!(retained > 0);
    pending.clear();
    assert!(pending.can_send());
    assert_eq!(pending.resident_bytes(), retained);
}

#[wasm_bindgen_test]
fn full_queues_do_not_reintroduce_locked_or_superseded_training_buttons() {
    let mut building = barracks(1, 15);
    let mut player = GameplayPlayerState {
        player: PlayerId(0),
        stockpile: Stockpile {
            food: 10000,
            gold: 10000,
            ..Default::default()
        },
        age: Age::Castle,
        researched: aoe_protocol::Tech::FeudalAge.bit() | aoe_protocol::Tech::CastleAge.bit(),
        population_used: 5,
        population_cap: 5,
        market_prices: Default::default(),
    };
    let mut pending = PendingTraining::default();
    assert!(matches!(
        blocked_reason(
            &building,
            Some(&player),
            PlayerId(0),
            UnitKind::ManAtArms,
            &pending
        ),
        Availability::Unavailable
    ));
    player.researched |= aoe_protocol::Tech::ManAtArms.bit();
    assert!(matches!(
        blocked_reason(
            &building,
            Some(&player),
            PlayerId(0),
            UnitKind::Militia,
            &pending
        ),
        Availability::Unavailable
    ));
    assert!(matches!(
        blocked_reason(
            &building,
            Some(&player),
            PlayerId(0),
            UnitKind::ManAtArms,
            &pending
        ),
        Availability::QueueFull
    ));
    building.training_queue.clear();
    for sequence in 0..aoe_protocol::MAX_PENDING_COMMANDS_PER_CONNECTION as u64 {
        pending.record(sequence, building.id);
    }
    assert!(matches!(
        blocked_reason(
            &building,
            Some(&player),
            PlayerId(0),
            UnitKind::Militia,
            &pending
        ),
        Availability::Unavailable
    ));
    assert!(matches!(
        blocked_reason(
            &building,
            Some(&player),
            PlayerId(0),
            UnitKind::ManAtArms,
            &pending
        ),
        Availability::QueueFull
    ));
}

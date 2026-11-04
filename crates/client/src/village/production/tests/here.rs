use super::super::here::{HereRequest, plan};
use super::*;
use aoe_core::{TileCoord, WorldPosition};

fn player() -> GameplayPlayerState {
    GameplayPlayerState {
        player: PlayerId(0),
        stockpile: Stockpile {
            food: 10000,
            gold: 10000,
            ..Default::default()
        },
        population_used: 4,
        population_cap: 100,
        age: Age::Dark,
        researched: 0,
        market_prices: Default::default(),
        recent_deliveries: [0; 4],
        unit_counts: [0; 34],
    }
}
fn request(count: usize) -> HereRequest {
    HereRequest {
        kind: UnitKind::Militia,
        count,
        position: WorldPosition::from_tile_center(TileCoord::new(12, 12)).unwrap(),
    }
}

#[wasm_bindgen_test]
fn units_here_balances_equally_near_queues_and_is_independent_of_input_order() {
    let mut buildings = vec![barracks(1, 2), barracks(2, 0), barracks(3, 1)];
    let expected = vec![
        EntityId(2),
        EntityId(2),
        EntityId(3),
        EntityId(1),
        EntityId(2),
    ];
    let choose = |b: &[GameplayBuildingState]| {
        plan(
            b,
            Some(&player()),
            PlayerId(0),
            request(5),
            &PendingTraining::default(),
            |_| false,
        )
    };
    assert_eq!(choose(&buildings), expected);
    buildings.reverse();
    assert_eq!(choose(&buildings), expected);
}

#[wasm_bindgen_test]
fn units_here_split_rejects_ineligible_and_pending_full_producers() {
    let mut buildings = vec![
        barracks(1, 15),
        barracks(2, 0),
        barracks(3, 0),
        barracks(4, 0),
        barracks(5, 14),
    ];
    buildings[1].construction = ConstructionState::UnderConstruction;
    buildings[2].player = PlayerId(1);
    buildings[3].kind = BuildingKind::Stable;
    let choose = |pending: &PendingTraining| {
        plan(
            &buildings,
            Some(&player()),
            PlayerId(0),
            request(10),
            pending,
            |_| false,
        )
    };
    assert_eq!(choose(&PendingTraining::default()), vec![EntityId(5)]);
    let mut pending = PendingTraining::default();
    pending.record(1, EntityId(5), UnitKind::Militia);
    assert!(choose(&pending).is_empty());
}

#[wasm_bindgen_test]
fn units_here_split_reserves_resources_and_population_for_queued_and_unobserved_orders() {
    let mut buildings = vec![barracks(1, 0), barracks(2, 0)];
    let mut p = player();
    p.stockpile.food = 120;
    p.stockpile.gold = 40;
    let choose =
        |b: &[GameplayBuildingState], p: &GameplayPlayerState, pending: &PendingTraining| {
            plan(b, Some(p), PlayerId(0), request(10), pending, |_| false)
        };
    let mut pending = PendingTraining::default();
    assert_eq!(choose(&buildings, &p, &pending).len(), 2);
    pending.record(1, EntityId(1), UnitKind::Militia);
    assert_eq!(choose(&buildings, &p, &pending).len(), 1);
    p.population_cap = 5;
    assert!(choose(&buildings, &p, &pending).is_empty());
    assert_eq!(choose(&buildings, &p, &PendingTraining::default()).len(), 1);
    buildings[0] = barracks(1, 1);
    assert!(choose(&buildings, &p, &PendingTraining::default()).is_empty());
    p.population_cap = 100;
    p.stockpile.food = 60;
    p.stockpile.gold = 20;
    pending.acknowledge(1, CommandResult::Accepted, 10, 10);
    pending.observe(10);
    assert_eq!(
        choose(&buildings, &p, &pending).len(),
        1,
        "observed costs must not be charged twice"
    );
}

#[wasm_bindgen_test]
fn units_here_split_bounds_a_burst_including_one_rally_per_producer() {
    let buildings: Vec<_> = (1..=40).map(|id| barracks(id, 0)).collect();
    let p = player();
    let orders = plan(
        &buildings,
        Some(&p),
        PlayerId(0),
        request(usize::MAX),
        &PendingTraining::default(),
        |_| false,
    );
    assert_eq!(orders.len(), aoe_protocol::HUMAN_COMMAND_BURST / 2);
    assert!(
        plan(
            &buildings,
            None,
            PlayerId(0),
            request(5),
            &PendingTraining::default(),
            |_| false
        )
        .is_empty()
    );
}

#[wasm_bindgen_test]
fn units_here_prioritizes_distance_even_inside_the_old_neighborhood() {
    let mut near = barracks(1, 1);
    near.footprint = TileRect::from_xywh(10, 10, 3, 3);
    let mut far = barracks(2, 0);
    far.footprint = TileRect::from_xywh(16, 10, 3, 3);
    assert_eq!(
        plan(
            &[far, near],
            Some(&player()),
            PlayerId(0),
            request(1),
            &PendingTraining::default(),
            |_| false
        ),
        vec![EntityId(1)]
    );
}

#[wasm_bindgen_test]
fn units_here_respects_the_human_twenty_command_burst_including_rallies() {
    let buildings: Vec<_> = (1..=40).map(|id| barracks(id, 0)).collect();
    let orders = plan(
        &buildings,
        Some(&player()),
        PlayerId(0),
        request(50),
        &PendingTraining::default(),
        |_| false,
    );
    let unique: std::collections::BTreeSet<_> = orders.iter().copied().collect();
    assert!(orders.len() + unique.len() <= 20);
}

#[wasm_bindgen_test]
fn units_here_uses_the_next_nearest_only_after_nearer_queues_fill() {
    let mut near = barracks(1, 14);
    near.footprint = TileRect::from_xywh(10, 10, 3, 3);
    let mut far = barracks(2, 0);
    far.footprint = TileRect::from_xywh(16, 10, 3, 3);
    assert_eq!(
        plan(
            &[far, near],
            Some(&player()),
            PlayerId(0),
            request(3),
            &PendingTraining::default(),
            |_| false
        ),
        vec![EntityId(1), EntityId(2), EntityId(2)]
    );
}

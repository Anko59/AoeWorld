use super::super::commerce::report;
use super::buildings::building;
use aoe_core::PlayerId;
use aoe_protocol::{Age, GameplayPlayerState, Stockpile, Tech, TrainingState, UnitKind};
use wasm_bindgen_test::wasm_bindgen_test;

fn player() -> GameplayPlayerState {
    GameplayPlayerState {
        player: PlayerId(0),
        stockpile: Stockpile {
            food: 10_000,
            wood: 10_000,
            gold: 10_000,
            stone: 10_000,
        },
        population_used: 4,
        population_cap: 100,
        age: Age::Dark,
        researched: 0,
        market_prices: Default::default(),
        recent_deliveries: [0; 4],
        unit_counts: [0; UnitKind::ALL.len()],
        held_tiles: 0,
        held_share_ppm: 0,
    }
}

fn training(kind: UnitKind) -> TrainingState {
    TrainingState {
        kind,
        progress_ticks: 0,
        total_ticks: 420,
    }
}

fn feudal_slot() -> usize {
    crate::building_ui::content::rules()
        .techs
        .iter()
        .position(|r| r.tech == Tech::FeudalAge as u8)
        .unwrap()
}

#[wasm_bindgen_test]
fn research_goes_to_the_idle_town_center_not_the_first_busy_one() {
    let mut busy = building(1);
    busy.training_queue = vec![training(UnitKind::Villager)];
    let idle = building(2);
    let player = player();
    for buildings in [vec![busy.clone(), idle.clone()], vec![idle, busy]] {
        let v = report(&buildings, Some(&player), PlayerId(0), u32::MAX);
        let at = 4 + feudal_slot() * 2;
        assert_eq!((v[at], v[at + 1]), (1.0, 2.0));
    }
}

#[wasm_bindgen_test]
fn unit_report_is_independent_of_selection_and_names_missing_producers() {
    let tc = building(1);
    let player = player();
    let v = report(&[tc.clone()], Some(&player), PlayerId(0), u32::MAX);
    let units = &v[v.len() - UnitKind::ALL.len()..];
    assert_eq!(units[UnitKind::Villager as usize], 1.0);
    // Nothing owned trains Militia.
    assert_eq!(units[UnitKind::Militia as usize], 12.0);
    let mut occupied = tc;
    occupied.training_queue = vec![training(UnitKind::Steward)];
    let v = report(&[occupied], Some(&player), PlayerId(0), u32::MAX);
    assert_ne!(
        v[v.len() - UnitKind::ALL.len() + UnitKind::Steward as usize],
        1.0
    );
}

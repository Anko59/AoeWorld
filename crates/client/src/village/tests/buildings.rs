use crate::playground::game::network::merge_buildings;
use aoe_core::{EntityId, PlayerId, TileCoord, TileRect};
use aoe_protocol::{BuildingKind, ConstructionState, GameplayBuildingState, MAX_ECONOMY_BUILDINGS};
use wasm_bindgen_test::wasm_bindgen_test;

fn building(id: usize) -> GameplayBuildingState {
    GameplayBuildingState {
        id: EntityId(id as u32),
        player: PlayerId(0),
        kind: BuildingKind::TownCenter,
        footprint: TileRect {
            min: TileCoord::new(0, 0),
            max: TileCoord::new(4, 4),
        },
        construction: ConstructionState::Complete,
        progress_permille: 1000,
        hp: 2400,
        max_hp: 2400,
        los: 8,
        food_remaining: None,
        research: None,
        training_queue: vec![],
        last_hit: None,
        gate_open: false,
        rally_point: None,
        farm_reseeds: 0,
        garrison: Vec::new(),
        bridge_span: None,
        age: aoe_protocol::Age::Dark,
    }
}

#[wasm_bindgen_test]
fn building_deltas_enforce_the_merged_limit_and_allow_replacements_and_removals() {
    // The complete retained-world cap is independent of the wire-message cap.
    // Keep the actual boundary exercised, including atomic overflow rejection.
    const LIMIT: usize = aoe_protocol::MAX_RETAINED_BUILDINGS;
    let retained: Vec<_> = (0..LIMIT).map(building).collect();
    assert!(merge_buildings(&retained, &[building(LIMIT + 99)], &[]).is_none());
    assert_eq!(retained.len(), LIMIT);
    let replaced = merge_buildings(&retained, &[building(0)], &[]).unwrap();
    assert_eq!(replaced.len(), LIMIT);
    drop(replaced);
    let swapped = merge_buildings(&retained, &[building(LIMIT + 99)], &[EntityId(0)]).unwrap();
    assert_eq!(swapped.len(), LIMIT);
    assert!(
        swapped
            .iter()
            .any(|b| b.id == EntityId((LIMIT + 99) as u32))
    );
    assert!(!swapped.iter().any(|b| b.id == EntityId(0)));
    assert!(merge_buildings(&swapped, &[building(LIMIT + 100)], &[]).is_none());
}

#[wasm_bindgen_test]
fn retained_buildings_can_span_multiple_bounded_messages() {
    let retained: Vec<_> = (0..MAX_ECONOMY_BUILDINGS).map(building).collect();
    let merged =
        merge_buildings(&retained, &[building(MAX_ECONOMY_BUILDINGS)], &[]).expect("another batch");
    assert_eq!(merged.len(), MAX_ECONOMY_BUILDINGS + 1);
}

fn rich_player(researched: u64) -> aoe_protocol::GameplayPlayerState {
    aoe_protocol::GameplayPlayerState {
        player: PlayerId(0),
        age: aoe_protocol::Age::Castle,
        researched,
        stockpile: aoe_protocol::Stockpile {
            food: 10000,
            wood: 10000,
            gold: 10000,
            stone: 10000,
        },
        population_used: 5,
        population_cap: 5,
        market_prices: Default::default(),
    }
}

#[wasm_bindgen_test]
fn training_buttons_allow_fifteen_entries_even_at_the_population_cap() {
    use crate::building_ui::availability::training;
    let mut b = building(1);
    let player = rich_player(0);
    for _ in 0..15 {
        assert_eq!(
            training(
                &b,
                Some(&player),
                PlayerId(0),
                aoe_protocol::UnitKind::Villager
            )
            .number(),
            1.0
        );
        b.training_queue.push(aoe_protocol::TrainingState {
            kind: aoe_protocol::UnitKind::Villager,
            progress_ticks: 0,
            total_ticks: 500,
        });
    }
    assert_eq!(
        training(
            &b,
            Some(&player),
            PlayerId(0),
            aoe_protocol::UnitKind::Villager
        )
        .number(),
        10.0
    );
}

#[wasm_bindgen_test]
fn production_lines_expose_only_the_current_unit_and_next_technology() {
    use crate::building_ui::{
        availability::{primary_training, research, training},
        content,
    };
    use aoe_protocol::{Tech, UnitKind};
    let mut b = building(1);
    b.kind = BuildingKind::Barracks;
    for (mask, current, next) in [
        (0, UnitKind::Militia, Some(Tech::ManAtArms)),
        (
            Tech::ManAtArms.bit(),
            UnitKind::ManAtArms,
            Some(Tech::LongSwordsman),
        ),
        (
            Tech::ManAtArms.bit() | Tech::LongSwordsman.bit(),
            UnitKind::LongSwordsman,
            None,
        ),
    ] {
        let player = rich_player(mask);
        assert_eq!(primary_training(&b, Some(&player)), current);
        for unit in [
            UnitKind::Militia,
            UnitKind::ManAtArms,
            UnitKind::LongSwordsman,
        ] {
            assert_eq!(
                training(&b, Some(&player), PlayerId(0), unit).number() == 1.0,
                unit == current
            );
        }
        for tech in [Tech::ManAtArms, Tech::LongSwordsman] {
            let status = research(
                &b,
                Some(&player),
                PlayerId(0),
                u32::MAX,
                false,
                &content::rules().techs[tech as usize],
            )
            .number();
            assert_eq!(status == 1.0, Some(tech) == next);
            if Some(tech) != next {
                assert!(status == 0.0 || status == 3.0);
            }
        }
    }
}

#[wasm_bindgen_test]
fn age_technology_line_exposes_only_its_current_step() {
    use crate::building_ui::{availability::research, content};
    use aoe_protocol::{Age, Tech};
    let b = building(1);
    for (age, mask, next) in [
        (Age::Dark, 0, Some(Tech::FeudalAge)),
        (Age::Feudal, Tech::FeudalAge.bit(), Some(Tech::CastleAge)),
        (
            Age::Castle,
            Tech::FeudalAge.bit() | Tech::CastleAge.bit(),
            None,
        ),
    ] {
        let mut player = rich_player(mask);
        player.age = age;
        for tech in [Tech::FeudalAge, Tech::CastleAge] {
            let status = research(
                &b,
                Some(&player),
                PlayerId(0),
                u32::MAX,
                false,
                &content::rules().techs[tech as usize],
            )
            .number();
            assert_eq!(status == 1.0, Some(tech) == next);
            if Some(tech) != next {
                assert!(status == 0.0 || status == 3.0);
            }
        }
    }
}

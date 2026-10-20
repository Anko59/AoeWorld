use super::*;
use aoe_core::TileCoord;
use aoe_protocol::BuildingKind as B;
use aoe_protocol::Stockpile;

#[wasm_bindgen_test::wasm_bindgen_test]
fn feudal_menu_gates_and_art_follow_the_owning_players_age() {
    use aoe_protocol::Age;
    let stock = Stockpile {
        wood: 1000,
        stone: 1000,
        ..Stockpile::default()
    };
    for kind in [
        B::ArcheryRange,
        B::Stable,
        B::Blacksmith,
        B::Market,
        B::WatchTower,
    ] {
        assert!(!build_enabled(kind, stock, u16::MAX, Age::Dark));
        assert!(build_enabled(kind, stock, u16::MAX, Age::Feudal));
    }
    for kind in [B::TownCenter, B::House, B::Mill, B::Barracks, B::Dock] {
        assert_ne!(
            age_sprite_slot(kind, 1000, false, Age::Dark),
            age_sprite_slot(kind, 1000, false, Age::Feudal)
        );
        assert_eq!(
            age_sprite_slot(kind, 500, false, Age::Dark),
            age_sprite_slot(kind, 500, false, Age::Feudal)
        );
    }
}

#[wasm_bindgen_test::wasm_bindgen_test]
fn menu_uses_costs_and_completed_mill_requirement() {
    let stock = Stockpile {
        wood: 100,
        stone: 4,
        ..Stockpile::default()
    };
    assert!(build_enabled(B::House, stock, 0, aoe_protocol::Age::Dark));
    assert!(!build_enabled(
        B::Barracks,
        stock,
        0,
        aoe_protocol::Age::Dark
    ));
    assert!(!build_enabled(
        B::Outpost,
        stock,
        0,
        aoe_protocol::Age::Dark
    ));
    assert!(!build_enabled(B::Farm, stock, 0, aoe_protocol::Age::Dark));
    assert!(build_enabled(
        B::Farm,
        stock,
        1 << B::Mill as u8,
        aoe_protocol::Age::Dark
    ));
    assert!(!build_enabled(
        B::TownCenter,
        stock,
        0,
        aoe_protocol::Age::Dark
    ));
}

#[wasm_bindgen_test::wasm_bindgen_test]
fn preview_checks_every_footprint_tile_and_map_bounds() {
    let origin = TileCoord::new(3, 4);
    let mut visited = Vec::new();
    assert!(footprint_valid(B::House, origin, [8, 8], |tile| {
        visited.push(tile);
        true
    }));
    assert_eq!(visited.len(), 4);
    for blocked in visited {
        assert!(!footprint_valid(B::House, origin, [8, 8], |tile| tile != blocked));
    }
    assert!(!footprint_valid(
        B::Farm,
        TileCoord::new(6, 6),
        [8, 8],
        |_| true
    ));
    assert!(!footprint_valid(
        B::House,
        TileCoord::new(-1, 0),
        [8, 8],
        |_| true
    ));
}

#[wasm_bindgen_test::wasm_bindgen_test]
fn art_selection_distinguishes_every_kind_and_construction_stage() {
    for rule in aoe_simulation::BUILDING_RULES {
        let kind = B::ALL[rule.kind as usize];
        if kind == B::Dock {
            assert_eq!(sprite_slot(kind, 1000, false), 23);
            assert_eq!(sprite_slot(kind, 0, false), 24);
            assert_eq!(sprite_slot(kind, 999, false), 24);
            assert_eq!(sprite_slot(kind, 1000, true), 25);
            continue;
        }
        assert_eq!(
            sprite_slot(kind, 1000, false),
            if rule.kind as usize <= 8 {
                rule.kind as usize
            } else {
                16 + rule.kind as usize
            }
        );
        assert_eq!(
            sprite_slot(kind, 0, false),
            if kind == B::PalisadeGate {
                43
            } else {
                9 + rule.kind.rules().side as usize - 1
            }
        );
        assert_eq!(
            sprite_slot(kind, 999, false),
            if kind == B::PalisadeGate {
                43
            } else {
                9 + rule.kind.rules().side as usize - 1
            }
        );
        assert_eq!(
            sprite_slot(kind, 1000, true),
            if rule.kind as usize <= 8 {
                13 + rule.kind as usize
            } else {
                22 + rule.kind as usize
            }
        );
    }
}

#[wasm_bindgen_test::wasm_bindgen_test]
fn walls_include_both_endpoints_and_refuse_unbounded_lines() {
    assert_eq!(
        wall_tiles(TileCoord::new(0, 0), TileCoord::new(2, 2)).unwrap(),
        vec![
            TileCoord::new(0, 0),
            TileCoord::new(1, 1),
            TileCoord::new(2, 2)
        ]
    );
    assert!(wall_tiles(TileCoord::new(0, 0), TileCoord::new(64, 0)).is_none());
}

#[wasm_bindgen_test::wasm_bindgen_test]
fn palisade_frames_follow_the_projected_line_and_junctions() {
    let at = TileCoord::new(4, 4);
    for (offset, frame) in [((1, 0), 1), ((0, 1), 0), ((1, 1), 4), ((1, -1), 3)] {
        assert_eq!(
            wall_frame(at, |t| t == TileCoord::new(4 + offset.0, 4 + offset.1)),
            frame
        );
    }
    assert_eq!(
        wall_frame(at, |t| t == TileCoord::new(5, 4)
            || t == TileCoord::new(4, 5)),
        2
    );
    assert_eq!(wall_frame(at, |_| false), 2);
}

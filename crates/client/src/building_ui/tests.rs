use super::*;
use aoe_core::TileCoord;
use aoe_protocol::BuildingKind as B;
use aoe_protocol::Stockpile;

#[wasm_bindgen_test::wasm_bindgen_test]
fn gate_art_uses_authoritative_state_without_any_resident_unit_input() {
    assert_eq!(gate_sprite_slot(true, 1000), Some(42));
    assert_eq!(gate_sprite_slot(false, 1000), None);
    assert_eq!(gate_sprite_slot(true, 500), None);
}

#[wasm_bindgen_test::wasm_bindgen_test]
fn unchanged_gate_art_needs_no_recomputation_and_hidden_gates_are_skipped() {
    let mut cache = GateArt::default();
    assert_eq!(
        cache.observe(aoe_core::EntityId(7), true, 1000, true),
        Some(42)
    );
    assert_eq!(cache.updates(), 1);
    for _ in 0..1000 {
        assert_eq!(
            cache.observe(aoe_core::EntityId(7), true, 1000, true),
            Some(42)
        );
    }
    assert_eq!(cache.updates(), 1);
    assert_eq!(
        cache.observe(aoe_core::EntityId(8), false, 1000, false),
        None
    );
    assert_eq!(cache.updates(), 1);
}

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
        assert!(!build_enabled(kind, stock, u32::MAX, Age::Dark));
        assert!(build_enabled(kind, stock, u32::MAX, Age::Feudal));
    }
    assert!(!build_enabled(B::Castle, stock, u32::MAX, Age::Feudal));
    assert!(build_enabled(B::Castle, stock, u32::MAX, Age::Castle));
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
        if kind == B::Bridge {
            assert_eq!(sprite_slot(kind, 1000, false), 66);
            assert_eq!(sprite_slot(kind, 0, false), 9);
            assert_eq!(sprite_slot(kind, 999, false), 9);
            // Collapsed bridge decks emit no rubble; this slot is never drawn.
            assert_eq!(sprite_slot(kind, 1000, true), 20);
            continue;
        }
        if kind == B::Dock {
            assert_eq!(sprite_slot(kind, 1000, false), 23);
            assert_eq!(sprite_slot(kind, 0, false), 24);
            assert_eq!(sprite_slot(kind, 999, false), 24);
            assert_eq!(sprite_slot(kind, 1000, true), 25);
            continue;
        }
        // Combined catalog retains four Castle buildings before six bridge pieces.
        if let Some((_, ready, rubble, foundation)) = [
            (B::Castle, 45, 49, 12),
            (B::Monastery, 46, 50, 11),
            (B::University, 47, 51, 12),
            (B::SiegeWorkshop, 48, 52, 12),
        ]
        .into_iter()
        .find(|(entry, _, _, _)| *entry == kind)
        {
            assert_eq!(sprite_slot(kind, 1000, false), ready);
            assert_eq!(sprite_slot(kind, 0, false), foundation);
            assert_eq!(sprite_slot(kind, 999, false), foundation);
            assert_eq!(sprite_slot(kind, 1000, true), rubble);
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

#[wasm_bindgen_test::wasm_bindgen_test]
fn market_panel_enables_only_affordable_trades_at_a_completed_owned_market() {
    use aoe_protocol::MarketPrices;
    let prices = MarketPrices {
        wood: 100,
        food: 103,
        stone: 90,
    };
    let have = Stockpile {
        food: 99,
        wood: 100,
        gold: 130,
        stone: 500,
    };
    // Buy costs price plus 30%: wood 130 is affordable, food 133 and stone 117
    // (90 * 1.3) are judged against 130 gold: only food is not. Sell pays
    // price minus 30%: 70, 72 (103 * 0.7 floored) and 63.
    assert_eq!(
        market_panel(true, prices, have),
        [
            100.0, 103.0, 90.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0, 130.0, 133.0, 117.0, 70.0, 72.0, 63.0
        ]
    );
    // Not a completed owned Market: prices and amounts stay visible, every
    // button is off.
    assert_eq!(
        market_panel(false, prices, have),
        [
            100.0, 103.0, 90.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 130.0, 133.0, 117.0, 70.0, 72.0, 63.0
        ]
    );
}

#[wasm_bindgen_test::wasm_bindgen_test]
fn market_rules_and_protocol_share_one_price_clamp() {
    let rules = aoe_simulation::MARKET_RULES;
    assert_eq!(rules.min_price, aoe_protocol::MARKET_MIN_PRICE);
    assert_eq!(rules.max_price, aoe_protocol::MARKET_MAX_PRICE);
    // Trades clamp to the same range the protocol validates against.
    assert_eq!(rules.lower(rules.min_price), aoe_protocol::MARKET_MIN_PRICE);
    assert_eq!(rules.raise(rules.max_price), aoe_protocol::MARKET_MAX_PRICE);
}

#[wasm_bindgen_test::wasm_bindgen_test]
fn town_center_menu_requires_castle_age_and_full_wood_and_stone_cost() {
    let stock = Stockpile {
        wood: 275,
        stone: 100,
        ..Default::default()
    };
    for age in [aoe_protocol::Age::Dark, aoe_protocol::Age::Feudal] {
        assert!(!build_enabled(B::TownCenter, stock, 0, age));
    }
    assert!(build_enabled(
        B::TownCenter,
        stock,
        0,
        aoe_protocol::Age::Castle
    ));
    assert!(!build_enabled(
        B::TownCenter,
        Stockpile { stone: 99, ..stock },
        0,
        aoe_protocol::Age::Castle
    ));
    assert!(!build_enabled(
        B::TownCenter,
        Stockpile { wood: 274, ..stock },
        0,
        aoe_protocol::Age::Castle
    ));
}

#[wasm_bindgen_test::wasm_bindgen_test]
fn bridge_caps_follow_the_native_axis_even_when_the_raster_slopes_backwards() {
    use aoe_rendering::SceneUnitKind as K;
    for (end, first, last) in [
        (TileCoord::new(32, 17), K::BridgeBTop, K::BridgeBBottom),
        (TileCoord::new(31, 17), K::BridgeABottom, K::BridgeATop),
        (TileCoord::new(31, 23), K::BridgeATop, K::BridgeABottom),
    ] {
        let from = TileCoord::new(29, 20);
        for (a, b) in [(from, end), (end, from)] {
            let tiles = aoe_core::raster_bridge_line(a, b).unwrap();
            assert_eq!(bridge_piece(&tiles, 0), first);
            assert_eq!(bridge_piece(&tiles, tiles.len() - 1), last);
        }
    }
}

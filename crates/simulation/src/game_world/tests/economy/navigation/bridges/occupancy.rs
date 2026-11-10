//! Occupancy agrees with exact footprints, including nonblocking structures.
use super::*;
use aoe_core::BridgeSpan;

fn agrees(w: &GameWorld) {
    for rise in -5..=5 {
        for y in 15..30 {
            let span =
                BridgeSpan::new(TileCoord::new(26, y), TileCoord::new(38, y + rise)).unwrap();
            let tiles = span.tiles();
            let scan = w
                .buildings()
                .any(|b| tiles.iter().any(|&t| b.contains_tile(t)));
            let indexed = tiles.iter().any(|&t| w.building_at(t).is_some());
            assert_eq!(indexed, scan, "{span:?}");
        }
    }
}

#[test]
fn bridge_occupancy_matches_exact_scan_through_insert_rebuild_and_remove() {
    let mut w = river_width(true, 30..34);
    w.set_fog_disabled(true);
    for complete in [false, true] {
        for kind in [
            BuildingKind::Road,
            BuildingKind::Farm,
            BuildingKind::PalisadeWall,
            BuildingKind::StoneGate,
        ] {
            let id = w
                .insert_building(
                    PlayerId(1),
                    kind,
                    TileRect::from_xywh(29, 20, 1, 4),
                    complete,
                )
                .unwrap();
            agrees(&w);
            w.rebuild_building_sight_index();
            agrees(&w);
            w.remove_building(id);
            agrees(&w);
        }
        let span = BridgeSpan::new(TileCoord::new(29, 20), TileCoord::new(34, 23)).unwrap();
        let id = w
            .insert_building_shape(
                PlayerId(1),
                BuildingKind::Bridge,
                span.footprint(),
                complete,
                Some(span),
            )
            .unwrap();
        agrees(&w);
        assert!(w.building_at(TileCoord::new(29, 23)).is_none());
        w.rebuild_building_sight_index();
        agrees(&w);
        w.remove_building(id);
        agrees(&w);
    }
}

#[test]
fn bridge_placement_rejects_nonblocking_road_and_foundation_on_its_span() {
    let mut w = river_width(true, 30..34);
    clear_nodes(&mut w, TileRect::from_xywh(26, 16, 12, 12));
    let (from, to) = (TileCoord::new(29, 20), TileCoord::new(34, 20));
    assert!(w.bridge_site_valid(from, to));
    for complete in [false, true] {
        let id = w
            .insert_building(
                PlayerId(1),
                BuildingKind::Road,
                TileRect::from_xywh(29, 20, 1, 1),
                complete,
            )
            .unwrap();
        assert!(!w.bridge_site_valid(from, to));
        w.remove_building(id);
        assert!(w.bridge_site_valid(from, to));
    }
}

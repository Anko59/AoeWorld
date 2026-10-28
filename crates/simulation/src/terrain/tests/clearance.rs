use super::*;

fn reference(w: &GameWorld, kind: UnitKind, tile: TileCoord) -> u8 {
    let mut nearest = 15;
    for dy in -8_i32..=8 {
        for dx in -8_i32..=8 {
            let p = TileCoord::new(tile.x + dx, tile.y + dy);
            if !w.unit_tile_passable(kind, p, &|| false).unwrap()
                || w.economy.blocked_tiles.contains(&p)
            {
                nearest = nearest.min((2 * dx.abs().max(dy.abs())).saturating_sub(1).max(0) as u8);
            }
        }
    }
    nearest
}

#[test]
fn clearance_matches_brute_force_across_chunk_edges_and_domains() {
    let mut w = super::far_orders::wide_world();
    super::group_baseline::forest(&mut w, TileRect::from_xywh(62, 60, 3, 6));
    for kind in [UnitKind::Militia, UnitKind::Galley] {
        for y in [0, 1, 55, 60, 63, 64, 67, 72] {
            for x in [0, 1, 54, 60, 62, 63, 64, 65, 72] {
                let p = TileCoord::new(x, y);
                assert_eq!(
                    w.static_clearance(kind, p),
                    reference(&w, kind, p),
                    "{kind:?} {p:?}"
                );
            }
        }
    }
}

#[test]
fn clearance_building_updates_cached_neighbour_chunk_and_removal_restores_it() {
    let mut w = world();
    let p = TileCoord::new(63, 64);
    assert_eq!(w.static_clearance(UnitKind::Militia, p), 15);
    let building = w
        .insert_building(
            PlayerId(0),
            BuildingKind::House,
            TileRect::from_xywh(64, 64, 2, 2),
            true,
        )
        .unwrap();
    assert_eq!(w.static_clearance(UnitKind::Militia, p), 1);
    assert_eq!(
        w.static_clearance(UnitKind::Militia, TileCoord::new(64, 64)),
        0
    );
    w.remove_building(building);
    assert_eq!(w.static_clearance(UnitKind::Militia, p), 15);
}

#[test]
fn clearance_tree_depletion_opens_a_cached_chunk() {
    let mut w = super::far_orders::wide_world();
    let tile = TileCoord::new(64, 64);
    assert_eq!(w.static_clearance(UnitKind::Militia, tile), 15);
    let node = aoe_map::ResourceNode::starting(tile, aoe_map::ResourceKind::Wood).unwrap();
    if let Terrain::Map { generator, overlay } = &mut w.terrain {
        overlay.insert_starting_resource(generator, node).unwrap();
    }
    w.resource_collision_changed();
    assert_eq!(w.static_clearance(UnitKind::Militia, tile), 0);
    assert!(
        w.deplete_resource(node.id, u16::MAX)
            .unwrap()
            .became_nonblocking
    );
    assert_eq!(w.static_clearance(UnitKind::Militia, tile), 15);
}

#[test]
fn clearance_land_and_water_pages_are_independent_and_do_not_change_replays() {
    let w = super::navy::coast();
    let before = w.canonical_hash();
    let mut open_water = false;
    for y in [24, 48, 64, 96] {
        for x in [24, 48, 64, 96, 128, 160] {
            let tile = TileCoord::new(x, y);
            for kind in [UnitKind::Militia, UnitKind::Galley] {
                let value = w.static_clearance(kind, tile);
                assert_eq!(value, reference(&w, kind, tile));
                open_water |= kind == UnitKind::Galley && value > 0;
            }
        }
    }
    assert!(open_water);
    assert_eq!(w.canonical_hash(), before);
}

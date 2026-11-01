use super::*;

fn ring_distance(rect: aoe_core::TileRect, tile: TileCoord) -> i32 {
    let dx = tile.x - tile.x.clamp(rect.min.x - 1, rect.max.x);
    let dy = tile.y - tile.y.clamp(rect.min.y - 1, rect.max.y);
    dx * dx + dy * dy
}

fn blocked_repair() -> (GameWorld, EntityId, EntityId, EntityId, TileCoord) {
    let (mut w, vassal, old_workers) = farmland(1);
    w.remove_unit(old_workers[0]);
    let plot = farm(&mut w, 135, 105, true);
    let crop = w.building(plot).unwrap().footprint;
    let center = TileCoord::new(crop.min.x + 1, crop.min.y + 1);
    // Every 2x2 near-Mill footprint contains an odd/odd tile. Their walls
    // leave continuous one-tile even-row/even-column walking corridors.
    for y in center.y - 7..=center.y + 7 {
        for x in center.x - 7..=center.x + 7 {
            let tile = TileCoord::new(x, y);
            if x % 2 == 0 || y % 2 == 0 || crop.contains(tile) {
                continue;
            }
            w.validate_building_placement(BuildingKind::PalisadeWall, tile)
                .unwrap();
            w.insert_building(
                PlayerId(0),
                BuildingKind::PalisadeWall,
                aoe_core::TileRect::from_xywh(x, y, 1, 1),
                true,
            )
            .unwrap();
        }
    }
    for y in center.y - 8..=center.y + 7 {
        for x in center.x - 8..=center.x + 7 {
            let tile = TileCoord::new(x, y);
            let rect = aoe_core::TileRect::from_xywh(x, y, 2, 2);
            if ring_distance(rect, center) <= 36 {
                assert!(
                    w.validate_building_placement(BuildingKind::Mill, tile)
                        .is_err(),
                    "fixture accidentally left a close Mill site at {tile:?}"
                );
            }
        }
    }
    let distant = TileCoord::new(center.x + 8, center.y);
    assert!(
        ring_distance(
            aoe_core::TileRect::from_xywh(distant.x, distant.y, 2, 2),
            center
        ) > 36
    );
    w.validate_building_placement(BuildingKind::Mill, distant)
        .unwrap();
    for x in center.x..distant.x {
        assert!(
            w.tile_passable(TileCoord::new(x, center.y)),
            "walking corridor must stay open"
        );
    }
    let worker = w
        .spawn_villager(
            PlayerId(0),
            WorldPosition::from_tile_center(center).unwrap(),
        )
        .unwrap();
    w.issue_farm(PlayerId(0), worker, plot).unwrap();
    let original_mills: Vec<_> = w
        .player_buildings_of(PlayerId(0), BuildingKind::Mill)
        .map(|b| b.footprint.min)
        .collect();
    assert_eq!(original_mills, vec![TileCoord::new(105, 105)]);
    (w, vassal, worker, plot, distant)
}

#[test]
fn farm_dropoff_repair_does_not_buy_a_distant_mill_when_only_a_corridor_reaches_it() {
    // Prove ordinary construction really admits the tempting far site and
    // reaches it through the legal corridor; blocked pathing cannot hide a buy.
    let (mut probe, _, worker, _, distant) = blocked_repair();
    let bought = probe
        .issue_build(PlayerId(0), &[worker], BuildingKind::Mill, distant, false)
        .unwrap();
    advance(&mut probe, 1_000);
    assert!(probe.building(bought).unwrap().work_done > 0);

    let (mut w, vassal, worker, plot, _) = blocked_repair();
    let original_mills: BTreeSet<_> = w
        .player_buildings_of(PlayerId(0), BuildingKind::Mill)
        .map(|b| b.id)
        .collect();
    advance(&mut w, 2_000);
    let new_mills: Vec<_> = w
        .player_buildings_of(PlayerId(0), BuildingKind::Mill)
        .filter(|b| !original_mills.contains(&b.id))
        .map(|b| (b.id, b.footprint))
        .collect();
    assert!(
        new_mills.is_empty(),
        "repair bought Mills despite no legal drop-off within six tiles; IDs and footprints: {new_mills:?}"
    );
    assert_eq!(w.vassal_controller(worker), Some(vassal));
    assert_eq!(worker_farm(&w, worker), Some(plot));
}

fn prepay_reseeds(w: &mut GameWorld) {
    let mill = w
        .player_buildings_of(PlayerId(0), BuildingKind::Mill)
        .next()
        .unwrap()
        .id;
    w.issue_farm_reseed(PlayerId(0), mill, true).unwrap();
    w.issue_farm_reseed(PlayerId(0), mill, true).unwrap();
    assert_eq!(w.building(mill).unwrap().farm_reseeds, 2);
}

fn second_blocked_farmer(w: &mut GameWorld) -> EntityId {
    let tile = TileCoord::new(135, 125);
    w.validate_building_placement(BuildingKind::Farm, tile)
        .unwrap();
    let plot = farm(w, tile.x, tile.y, true);
    let crop = w.building(plot).unwrap().footprint;
    let center = TileCoord::new(tile.x + 1, tile.y + 1);
    for y in center.y - 7..=center.y + 7 {
        for x in center.x - 7..=center.x + 7 {
            let at = TileCoord::new(x, y);
            if x % 2 != 0 && y % 2 != 0 && !crop.contains(at) {
                w.validate_building_placement(BuildingKind::PalisadeWall, at)
                    .unwrap();
                w.insert_building(
                    PlayerId(0),
                    BuildingKind::PalisadeWall,
                    aoe_core::TileRect::from_xywh(x, y, 1, 1),
                    true,
                )
                .unwrap();
            }
        }
    }
    for y in center.y - 8..=center.y + 7 {
        for x in center.x - 8..=center.x + 7 {
            if ring_distance(aoe_core::TileRect::from_xywh(x, y, 2, 2), center) <= 36 {
                assert!(
                    w.validate_building_placement(BuildingKind::Mill, TileCoord::new(x, y))
                        .is_err()
                );
            }
        }
    }
    let distant = TileCoord::new(center.x + 8, center.y);
    w.validate_building_placement(BuildingKind::Mill, distant)
        .unwrap();
    for x in center.x..distant.x {
        assert!(w.tile_passable(TileCoord::new(x, center.y)));
    }
    let worker = w
        .spawn_villager(PlayerId(0), at(center.x, center.y))
        .unwrap();
    w.issue_farm(PlayerId(0), worker, plot).unwrap();
    worker
}

#[test]
fn blocked_repair_leaves_work_for_both_an_abandoned_plot_and_a_new_farm() {
    let (mut w, vassal, blocked_worker, _, _) = blocked_repair();
    prepay_reseeds(&mut w);
    let second_blocked = second_blocked_farmer(&mut w);
    let tile = TileCoord::new(108, 105);
    w.validate_building_placement(BuildingKind::Farm, tile)
        .unwrap();
    let free_plot = farm(&mut w, tile.x, tile.y, true);
    let reuse_worker = w.spawn_villager(PlayerId(0), at(109, 106)).unwrap();
    let new_worker = w.spawn_villager(PlayerId(0), at(112, 112)).unwrap();
    advance(&mut w, crate::VASSAL_RULES.period_ticks as u32 + 1);
    let reused = w.unit(reuse_worker).unwrap();
    assert_eq!(
        reused.task,
        UnitTask::Farming,
        "a blocked repair must not prevent claiming an existing farm; {}",
        farming_diagnostics(
            &w,
            &[blocked_worker, second_blocked, reuse_worker, new_worker]
        )
    );
    assert_eq!(w.vassal_controller(reuse_worker), Some(vassal));
    assert_eq!(worker_farm(&w, reuse_worker), Some(free_plot));
    assert!(
        w.building(free_plot)
            .unwrap()
            .footprint
            .contains(reused.position.tile_floor())
    );
    let building = w
        .economy
        .builders
        .get(&new_worker)
        .expect("a second idle worker must receive ordinary new-farm construction")
        .building;
    let foundation = w.building(building).unwrap();
    assert_eq!(foundation.kind, BuildingKind::Farm);
    assert!(!foundation.is_complete());
    assert_eq!(w.vassal_controller(new_worker), Some(vassal));
}

fn repairable_second() -> (GameWorld, EntityId, EntityId, TileCoord) {
    let (mut w, vassal, _, _, _) = blocked_repair();
    prepay_reseeds(&mut w);
    let tile = TileCoord::new(135, 125);
    w.validate_building_placement(BuildingKind::Farm, tile)
        .unwrap();
    let plot = farm(&mut w, tile.x, tile.y, true);
    let center = TileCoord::new(tile.x + 1, tile.y + 1);
    let worker = w
        .spawn_villager(PlayerId(0), at(center.x, center.y))
        .unwrap();
    w.issue_farm(PlayerId(0), worker, plot).unwrap();
    let repair_site = TileCoord::new(center.x - 3, center.y);
    w.validate_building_placement(BuildingKind::Mill, repair_site)
        .unwrap();
    assert!(w.tile_passable(TileCoord::new(center.x - 1, center.y)));
    (w, vassal, worker, center)
}

#[test]
fn an_impossible_first_repair_does_not_starve_a_second_farmer_before_its_first_grain() {
    let (mut a, a_vassal, a_worker, a_center) = repairable_second();
    let (mut b, b_vassal, b_worker, b_center) = repairable_second();
    for tick in 0..49 {
        a.advance();
        b.advance();
        assert_eq!(
            a.canonical_hash(),
            b.canonical_hash(),
            "repair replay tick {tick}"
        );
        assert_eq!(
            a.vassal_work(),
            b.vassal_work(),
            "repair replay tick {tick}"
        );
    }
    for (w, vassal, worker, center) in [
        (&a, a_vassal, a_worker, a_center),
        (&b, b_vassal, b_worker, b_center),
    ] {
        assert!(
            w.unit(worker).unwrap().carried.is_none(),
            "repair must get its turn before the first grain"
        );
        let close_mills: Vec<_> = w
            .player_buildings_of(PlayerId(0), BuildingKind::Mill)
            .filter(|b| !b.is_complete() && ring_distance(b.footprint, center) <= 36)
            .map(|b| b.id)
            .collect();
        assert!(
            !close_mills.is_empty(),
            "a blocked earlier farmer must not monopolize repair selection"
        );
        assert_eq!(w.vassal_controller(worker), Some(vassal));
    }
}

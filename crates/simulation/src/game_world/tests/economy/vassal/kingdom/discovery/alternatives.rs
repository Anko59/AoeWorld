//! A refused cached ore candidate must not starve reachable alternatives.
use super::*;

#[derive(Clone, Copy, Eq, PartialEq)]
enum Layout {
    Separated,
    CampOffer,
    IntraBlock,
}

struct Alternative {
    world: GameWorld,
    worker: EntityId,
    vassal: EntityId,
    castle: EntityId,
    farm: EntityId,
    farm_tile: TileCoord,
    blocked: Vec<ResourceNode>,
    reachable: ResourceNode,
}

fn alternative_fixture(kind: ResourceKind, bad_count: usize, layout: Layout) -> Alternative {
    let (package, pages, water) = fixture::flat_fixture(false);
    let generator = package
        .generator_with_environment(pages.clone(), water.clone(), Vec::new(), Vec::new())
        .unwrap();
    let pattern = if layout == Layout::IntraBlock {
        [
            (149, 132),
            (150, 132),
            (151, 132),
            (149, 133),
            (151, 133),
            (149, 134),
            (150, 134),
            (151, 134),
            (144, 128),
        ]
    } else {
        [
            (144, 126),
            (145, 126),
            (146, 126),
            (144, 127),
            (146, 127),
            (144, 128),
            (145, 128),
            (146, 128),
            (172, 128),
        ]
    };
    // Production startup insertion requires genuinely bare generator tiles,
    // even if a natural resource on a tile was depleted. Translate the entire
    // fixed layout together until its nine ore tiles are naturally bare.
    let translation = if layout == Layout::IntraBlock { 8 } else { 4 };
    let (dx, dy) = (0..=64)
        .step_by(translation)
        .flat_map(|dy| (0..=64).step_by(translation).map(move |dx| (dx, dy)))
        .find(|(dx, dy)| {
            pattern.iter().all(|(x, y)| {
                let at = TileCoord::new(x + dx, y + dy);
                generator.object_at(at).is_none()
                    && generator.tile_at(at).is_some_and(|tile| tile.passable)
            })
        })
        .expect("flat map needs nine bare tiles for the translated starting-resource layout");
    let home_center = TileCoord::new(152 + dx, 140 + dy);
    let start = TileCoord::new(
        if layout == Layout::IntraBlock {
            120
        } else {
            126
        } + dx,
        128 + dy,
    );
    let blocked: Vec<_> = pattern[..bad_count]
        .iter()
        .map(|(x, y)| ResourceNode::starting(TileCoord::new(x + dx, y + dy), kind).unwrap())
        .collect();
    let reachable =
        ResourceNode::starting(TileCoord::new(pattern[8].0 + dx, pattern[8].1 + dy), kind).unwrap();
    let mut natural = Vec::new();
    for y in (home_center.y - 102).div_euclid(32)..=(home_center.y + 102).div_euclid(32) {
        for x in (home_center.x - 102).div_euclid(32)..=(home_center.x + 102).div_euclid(32) {
            natural.extend(generator.chunk(x, y).unwrap().resources);
        }
    }
    let mut w =
        GameWorld::from_prepared_map(package, pages, water, Vec::new(), Vec::new()).unwrap();
    w.set_fog_disabled(true);
    // Clear the complete Home and all fixture sites through normal depletion.
    // Wood cannot win the zero-stock tie or buy a nearer Camp. The following
    // are real production starting nodes, with ordinary work and finite yield.
    for node in natural {
        w.deplete_resource(node.id, node.initial_amount).unwrap();
    }
    if let Terrain::Map { generator, overlay } = &mut w.terrain {
        for node in blocked.iter().chain(std::iter::once(&reachable)) {
            overlay.insert_starting_resource(generator, *node).unwrap();
        }
    } else {
        panic!("prepared map fixture");
    }
    // Match production starting-kit cache/collision refresh after insertion.
    if let Ok(cache) = w.resource_chunks.get_mut() {
        cache.clear();
    }
    w.resource_collision_changed();
    let mut walls = BTreeSet::new();
    for node in &blocked {
        for dy in -1..=1 {
            for dx in -1..=1 {
                let at = TileCoord::new(node.tile.x + dx, node.tile.y + dy);
                if blocked.iter().any(|a| a.tile == at) || !walls.insert((at.x, at.y)) {
                    continue;
                }
                w.validate_building_placement(BuildingKind::PalisadeWall, at)
                    .unwrap();
                w.insert_building(
                    PlayerId(0),
                    BuildingKind::PalisadeWall,
                    aoe_core::TileRect::from_xywh(at.x, at.y, 1, 1),
                    true,
                )
                .unwrap();
            }
        }
    }
    let mut building = |kind: BuildingKind, at: TileCoord| {
        w.validate_building_placement(kind, at).unwrap();
        w.insert_building(
            PlayerId(0),
            kind,
            aoe_core::TileRect::from_xywh(at.x, at.y, kind.rules().side, kind.rules().depth),
            true,
        )
        .unwrap()
    };
    let castle = building(
        BuildingKind::Castle,
        TileCoord::new(home_center.x - 2, home_center.y - 2),
    );
    let camp = building(
        BuildingKind::MiningCamp,
        if layout == Layout::CampOffer {
            TileCoord::new(153 + dx, 126 + dy)
        } else {
            TileCoord::new(start.x + 6, start.y - 1)
        },
    );
    let mill = building(BuildingKind::Mill, TileCoord::new(start.x + 3, start.y - 1));
    let farm = building(BuildingKind::Farm, TileCoord::new(start.x - 1, start.y - 1));
    let footprint = w.building(castle).unwrap().footprint;
    let center = TileCoord::new(footprint.min.x + 2, footprint.min.y + 2);
    assert_eq!(center, home_center);
    assert!(squared(center, start) < 100 * 100);
    for node in blocked.iter().chain(std::iter::once(&reachable)) {
        assert!(squared(center, node.tile) < 99 * 99);
        assert!(squared(start, node.tile) > 15 * 15);
        assert!(w.resource_remaining(node.id).unwrap() > 0);
        let rect = w.building(camp).unwrap().footprint;
        let dx = node.tile.x - node.tile.x.clamp(rect.min.x - 1, rect.max.x);
        let dy = node.tile.y - node.tile.y.clamp(rect.min.y - 1, rect.max.y);
        if layout == Layout::CampOffer && node.id != reachable.id {
            assert!(squared(rect.min, node.tile) <= 10 * 10);
        } else {
            assert!(
                dx * dx + dy * dy > 64,
                "the existing Camp is genuinely far away"
            );
        }
    }
    for node in &blocked {
        assert!(squared(center, node.tile) < squared(center, reachable.tile));
        if layout == Layout::IntraBlock {
            assert_eq!(
                (node.tile.x.div_euclid(8), node.tile.y.div_euclid(8)),
                (
                    reachable.tile.x.div_euclid(8),
                    reachable.tile.y.div_euclid(8)
                )
            );
            assert!((node.tile.y, node.tile.x) > (reachable.tile.y, reachable.tile.x));
        } else {
            assert!(squared(node.tile, reachable.tile) > 10 * 10);
            assert!(squared(start, node.tile) < squared(start, reachable.tile));
        }
    }
    if layout == Layout::CampOffer {
        let origin = w.building(camp).unwrap().footprint.min;
        assert_eq!(
            w.nearest_resource(origin, kind, 10, None).unwrap().id,
            blocked[0].id
        );
        assert!(squared(origin, reachable.tile) > 10 * 10);
    }
    assert!(w.nearest_resource(start, kind, 10, None).is_none());
    w.economy.players.get_mut(&PlayerId(0)).unwrap().age = Age::Castle;
    reserves(&mut w, 30_000, 30_000, 30_000, 30_000);
    for _ in 0..2 {
        w.issue_farm_reseed(PlayerId(0), mill, true).unwrap();
    }
    let worker = w.spawn_villager(PlayerId(0), position(start)).unwrap();
    w.issue_farm(PlayerId(0), worker, farm).unwrap();
    let before = resource_stock(&w, ResourceKind::Food);
    advance(&mut w, 4_000);
    assert!(
        resource_stock(&w, ResourceKind::Food) > before,
        "source Farm actually deposits"
    );
    assert_eq!(w.gather_tile(worker), Some(start));
    assert_eq!(w.gathering_kind(worker), Some(ResourceKind::Food));
    assert_eq!(w.vassal_controller(worker), None);
    for node in &blocked {
        assert!(
            !w.resource_has_work_spot(worker, node.tile),
            "legal walls block every work point"
        );
        w.advance(); // Fresh ordinary admission budgets for each independent refusal.
        let carried = w.unit(worker).unwrap().carried;
        assert!(w.issue_gather(PlayerId(0), worker, node.id).is_err());
        assert_eq!(
            w.gather_tile(worker),
            Some(start),
            "refusal preserves the old Farm"
        );
        assert_eq!(w.gathering_kind(worker), Some(ResourceKind::Food));
        assert_eq!(w.unit(worker).unwrap().carried, carried);
    }
    let vassal = w
        .spawn_kind(
            PlayerId(0),
            UnitKind::Vassal,
            position(TileCoord::new(footprint.min.x - 1, footprint.min.y)),
        )
        .unwrap();
    w.bind_vassal(vassal, castle, footprint);
    reserves(
        &mut w,
        30_000,
        0,
        if kind == ResourceKind::Gold {
            0
        } else {
            30_000
        },
        if kind == ResourceKind::Stone {
            0
        } else {
            30_000
        },
    );
    Alternative {
        world: w,
        worker,
        vassal,
        castle,
        farm,
        farm_tile: start,
        blocked,
        reachable,
    }
}

fn activate(s: &mut Alternative) {
    s.world
        .issue_garrison(PlayerId(0), s.vassal, s.castle)
        .unwrap();
    s.world.advance_garrisons(&mut Vec::new());
    assert!(s.world.vassal_status(s.vassal).is_some());
}

fn reachable_probe(kind: ResourceKind, bad_count: usize, layout: Layout) {
    let mut s = alternative_fixture(kind, bad_count, layout);
    s.world.advance();
    s.world
        .issue_gather(PlayerId(0), s.worker, s.reachable.id)
        .unwrap();
    assert_eq!(s.world.gather_target(s.worker), Some(s.reachable.id));
    for _ in 0..10_000 {
        s.world.advance();
        if resource_stock(&s.world, kind) >= u32::from(VILLAGE_RULES.villager_capacity) {
            assert!(s.world.vassal_status(s.vassal).is_none());
            return;
        }
    }
    panic!("independent ordinary worker must deposit from the reachable farther ore");
}

fn eventually_tries_reachable(kind: ResourceKind, bad_count: usize, layout: Layout) {
    reachable_probe(kind, bad_count, layout);
    let mut a = alternative_fixture(kind, bad_count, layout);
    let mut b = alternative_fixture(kind, bad_count, layout);
    activate(&mut a);
    activate(&mut b);
    let mut selected_good = false;
    for tick in 0..10_000 {
        a.world.advance();
        b.world.advance();
        assert_eq!(
            a.world.canonical_hash(),
            b.world.canonical_hash(),
            "alternative replay tick {tick}"
        );
        assert_eq!(a.world.vassal_work(), b.world.vassal_work());
        bounded(a.world.vassal_work());
        for s in [&a, &b] {
            let work = s.world.vassal_work();
            // A ten-tile query intersects at most a 5x5 grid of 8x8 blocks.
            assert!(work.resource_blocks <= work.resource_searches * 25);
            if s.world.gathering_kind(s.worker) != Some(kind) {
                if s.world.gathering_kind(s.worker) == Some(ResourceKind::Food) {
                    assert_eq!(s.world.gather_tile(s.worker), Some(s.farm_tile));
                } else {
                    // A prepaid ordinary reseed may briefly be construction,
                    // but a refused ore order cannot discard the original plot.
                    assert_eq!(
                        s.world
                            .economy
                            .builders
                            .get(&s.worker)
                            .map(|job| job.building),
                        Some(s.farm),
                        "refused ore admission must retain Food work or its reseed"
                    );
                }
            } else {
                assert_eq!(s.world.gather_target(s.worker), Some(s.reachable.id));
                selected_good = true;
            }
        }
        if selected_good
            && resource_stock(&a.world, kind) >= u32::from(VILLAGE_RULES.villager_capacity)
            && resource_stock(&b.world, kind) >= u32::from(VILLAGE_RULES.villager_capacity)
        {
            assert_eq!(a.world.vassal_controller(a.worker), Some(a.vassal));
            assert_eq!(b.world.vassal_controller(b.worker), Some(b.vassal));
            return;
        }
    }
    panic!(
        "{} blocked cached candidates hid reachable {:?}; stocks {}/{}, bad IDs {:?}",
        bad_count,
        a.reachable.tile,
        resource_stock(&a.world, kind),
        resource_stock(&b.world, kind),
        a.blocked.iter().map(|n| n.id).collect::<Vec<_>>()
    );
}

#[test]
fn refused_nearest_distant_gold_does_not_hide_a_reachable_alternative_or_discard_farming() {
    eventually_tries_reachable(ResourceKind::Gold, 1, Layout::Separated);
}

#[test]
fn eight_blocked_distant_stone_candidates_do_not_hide_a_farther_reachable_source() {
    eventually_tries_reachable(ResourceKind::Stone, 8, Layout::Separated);
}

#[test]
fn a_refused_distant_camp_offer_does_not_bypass_reachable_cached_alternatives() {
    eventually_tries_reachable(ResourceKind::Gold, 1, Layout::CampOffer);
}

#[test]
fn eight_later_nodes_in_one_block_do_not_permanently_evict_the_reachable_first_node() {
    eventually_tries_reachable(ResourceKind::Stone, 8, Layout::IntraBlock);
}

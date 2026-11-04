//! Real prepared-map queries stay bounded as active domains rotate.
use super::*;

fn dry_cells(generator: &aoe_map::MapChunkGenerator) -> Vec<(TileCoord, ResourceNode, bool)> {
    let dry = |at| {
        generator
            .tile_at(at)
            .is_some_and(|sample| sample.passable && sample.water == aoe_map::WaterKind::None)
    };
    let mut cells = Vec::new();
    'cells: for y in (16..generator.width_tiles() - 24).step_by(28) {
        for x in (16..generator.width_tiles() - 24).step_by(28) {
            let at = TileCoord::new(x, y);
            if ![(0, 0, 4, 4), (10, 2, 2, 2), (7, 7, 1, 1), (-1, 0, 1, 1)]
                .iter()
                .all(|(dx, dy, width, height)| {
                    (0..*height).all(|sy| {
                        (0..*width).all(|sx| dry(TileCoord::new(x + dx + sx, y + dy + sy)))
                    })
                })
            {
                continue;
            }
            let source = (13..=19)
                .flat_map(|dy| (16..=22).map(move |dx| TileCoord::new(x + dx, y + dy)))
                .find_map(|tile| {
                    if !dry(tile) {
                        return None;
                    }
                    match generator.object_at(tile) {
                        Some(node) if node.kind == ResourceKind::Wood => Some((node, false)),
                        None => Some((
                            ResourceNode::starting(tile, ResourceKind::Wood).unwrap(),
                            true,
                        )),
                        _ => None,
                    }
                });
            if let Some((node, startup)) = source {
                cells.push((at, node, startup));
                if cells.len() == 100 {
                    break 'cells;
                }
            }
        }
    }
    assert_eq!(
        cells.len(),
        100,
        "current prepared map needs100 dry disjoint cells for legal Town Centers/Camps and real sources"
    );
    cells
}

fn hundred_prepared_domains() -> (GameWorld, Vec<(EntityId, EntityId)>) {
    let (package, pages, water) = fixture::flat_fixture(false);
    let generator = package
        .generator_with_environment(pages.clone(), water.clone(), Vec::new(), Vec::new())
        .unwrap();
    let mut clearing = std::collections::BTreeMap::new();
    let cells = dry_cells(&generator);
    let mut trees = Vec::new();
    for (at, node, startup) in &cells {
        for dy in -2..=11 {
            for dx in -2..=13 {
                if let Some(node) = generator.object_at(TileCoord::new(at.x + dx, at.y + dy)) {
                    clearing.insert(node.id, node);
                }
            }
        }
        // A fully forested square already has the real work source this
        // fixture needs; do not require an artificial hole in that forest.
        trees.push((*node, *startup));
    }
    let mut w =
        GameWorld::from_prepared_map(package, pages, water, Vec::new(), Vec::new()).unwrap();
    w.set_fog_disabled(true);
    w.set_limits(crate::WorldLimits::for_match(100));
    for node in clearing.into_values() {
        w.deplete_resource(node.id, node.initial_amount).unwrap();
    }
    if let Terrain::Map { generator, overlay } = &mut w.terrain {
        for (node, startup) in &trees {
            if *startup {
                overlay.insert_starting_resource(generator, *node).unwrap();
            }
        }
    } else {
        panic!("prepared map fixture");
    }
    if let Ok(cache) = w.resource_chunks.get_mut() {
        cache.clear();
    }
    w.resource_collision_changed();
    assert_eq!(
        trees
            .iter()
            .map(|(node, _)| node.id)
            .collect::<BTreeSet<_>>()
            .len(),
        100
    );
    assert!(
        trees
            .iter()
            .all(|(node, _)| w.resource_remaining(node.id).is_some_and(|left| left > 0))
    );
    let mut domains = Vec::new();
    for (seat, (at, _, _)) in cells.into_iter().enumerate() {
        let owner = PlayerId(u16::try_from(seat).unwrap());
        let footprint = aoe_core::TileRect::from_xywh(at.x, at.y, 4, 4);
        w.validate_building_placement(BuildingKind::TownCenter, at)
            .unwrap();
        let town_center = w
            .insert_building(owner, BuildingKind::TownCenter, footprint, true)
            .unwrap();
        let camp = TileCoord::new(at.x + 10, at.y + 2);
        w.validate_building_placement(BuildingKind::LumberCamp, camp)
            .unwrap();
        w.insert_building(
            owner,
            BuildingKind::LumberCamp,
            aoe_core::TileRect::from_xywh(camp.x, camp.y, 2, 2),
            true,
        )
        .unwrap();
        let p = w.economy.players.get_mut(&owner).unwrap();
        p.age = Age::Castle;
        p.stockpile = Stockpile {
            food: 30_000,
            wood: 0,
            gold: 30_000,
            stone: 30_000,
        };
        let worker = w
            .spawn_villager(owner, position(TileCoord::new(at.x + 7, at.y + 7)))
            .unwrap();
        let steward = w
            .spawn_kind(
                owner,
                UnitKind::Steward,
                position(TileCoord::new(at.x - 1, at.y)),
            )
            .unwrap();

        // Adjacent ordinary garrisons need no unbounded setup movement or
        // manager passes before all hundred domains are available to rotate.
        w.issue_garrison(owner, steward, town_center).unwrap();
        domains.push((steward, worker));
    }
    w.advance_garrisons(&mut Vec::new());
    assert_eq!(domains.len(), 100);
    assert!(
        domains
            .iter()
            .all(|(steward, _)| w.steward_status(*steward).is_some())
    );
    (w, domains)
}

#[test]
fn a_hundred_prepared_map_domains_rotate_real_resource_queries_inside_finder_budgets_and_replay() {
    let (mut a, a_domains) = hundred_prepared_domains();
    let (mut b, b_domains) = hundred_prepared_domains();
    let mut observed = BTreeSet::new();
    let mut search_total = 0;
    let mut node_total = 0;
    let ticks = 2
        * 100_usize.div_ceil(crate::STEWARD_RULES.domains_per_pass)
        * crate::STEWARD_RULES.period_ticks as usize;
    for tick in 0..ticks {
        a.advance();
        b.advance();
        assert_eq!(
            a.canonical_hash(),
            b.canonical_hash(),
            "hundred-domain replay tick {tick}"
        );
        assert_eq!(a.steward_work(), b.steward_work());
        let work = a.steward_work();
        bounded(work);
        assert!(work.resource_blocks <= 25 * work.resource_searches);
        assert!(work.resource_nodes <= 1_608 * work.resource_searches);
        search_total += work.resource_searches;
        node_total += work.resource_nodes;
        for ((steward, _), (other, _)) in a_domains.iter().zip(&b_domains) {
            assert_eq!(a.steward_status(*steward), b.steward_status(*other));
            if a.steward_status(*steward).unwrap().residents == 1 {
                observed.insert(*steward);
            }
        }
    }
    assert_eq!(
        observed.len(),
        100,
        "every active Home receives its real worker census"
    );
    assert!(
        search_total >= 100,
        "rotation must exercise actual resource queries"
    );
    assert!(
        node_total >= 100,
        "live natural/starting nodes must actually be inspected"
    );
    assert!(
        a_domains.iter().any(|(steward, worker)| {
            a.steward_controller(*worker) == Some(*steward)
                && a.gathering_kind(*worker) == Some(ResourceKind::Wood)
        }),
        "real ordinary Wood work must result from the searches"
    );
    // These are returned-entry finder counters. Cold starting-history work is
    // separately spatial: 32 row/kind seeks, <=256 entries, <=64 tiles/block;
    // it is not a claimed measurement of global history or hardware RTS speed.
}

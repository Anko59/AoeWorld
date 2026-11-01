use super::*;
mod alternatives;
mod many_domains;

fn squared(a: TileCoord, b: TileCoord) -> u64 {
    let dx = i64::from(a.x - b.x);
    let dy = i64::from(a.y - b.y);
    (dx * dx + dy * dy) as u64
}

fn overlaps(a: aoe_core::TileRect, b: aoe_core::TileRect) -> bool {
    a.min.x < b.max.x && b.min.x < a.max.x && a.min.y < b.max.y && b.min.y < a.max.y
}

fn distant_ore(
    kind: ResourceKind,
    with_camp: bool,
) -> (GameWorld, EntityId, EntityId, ResourceNode) {
    let (package, pages, water) = fixture::flat_fixture(false);
    let generator = package
        .generator_with_environment(pages.clone(), water.clone(), Vec::new(), Vec::new())
        .unwrap();
    let mut nodes = Vec::new();
    for y in 0..8 {
        for x in 0..8 {
            nodes.extend(
                generator
                    .chunk(x, y)
                    .unwrap()
                    .resources
                    .into_iter()
                    .filter(|n| n.kind == kind),
            );
        }
    }
    let mut w =
        GameWorld::from_prepared_map(package, pages, water, Vec::new(), Vec::new()).unwrap();
    w.set_fog_disabled(true);
    let mut selected = None;
    'origin: for y in (16..240).step_by(8) {
        for x in (16..240).step_by(8) {
            let start = TileCoord::new(x, y);
            if !w.tile_free(start) {
                continue;
            }
            let Some(node) = nodes
                .iter()
                .min_by_key(|n| (squared(start, n.tile), n.id))
                .copied()
            else {
                continue;
            };
            let distance = squared(start, node.tile);
            // Put every nearest ore source outside the old ten-tile scan,
            // preferably at least two candidate blocks farther in both axes.
            if distance <= 10 * 10
                || distance > 60 * 60
                || node.tile.x.div_euclid(8) < start.x.div_euclid(8) + 2
                || node.tile.y.div_euclid(8) < start.y.div_euclid(8) + 2
            {
                continue;
            }
            for (dx, dy) in [
                (8, 0),
                (0, 8),
                (-8, 0),
                (0, -8),
                (8, 8),
                (-8, 8),
                (8, -8),
                (-8, -8),
            ] {
                let castle = TileCoord::new(x + dx, y + dy);
                if w.validate_building_placement(BuildingKind::Castle, castle)
                    .is_err()
                {
                    continue;
                }
                let castle_rect = aoe_core::TileRect::from_xywh(castle.x, castle.y, 4, 4);
                for (cx, cy) in [
                    (-4, 0),
                    (0, -4),
                    (4, 0),
                    (0, 4),
                    (-4, -4),
                    (-4, 4),
                    (4, -4),
                    (4, 4),
                ] {
                    let camp = TileCoord::new(x + cx, y + cy);
                    let camp_rect = aoe_core::TileRect::from_xywh(camp.x, camp.y, 2, 2);
                    if !overlaps(castle_rect, camp_rect)
                        && w.validate_building_placement(BuildingKind::MiningCamp, camp)
                            .is_ok()
                    {
                        selected = Some((start, node, castle_rect, camp_rect));
                        break 'origin;
                    }
                }
            }
        }
    }
    let (start, node, castle_rect, camp_rect) =
        selected.expect("seeded fixture needs visible distant ore and clear local buildings");
    assert!(
        nodes
            .iter()
            .all(|n| squared(start, n.tile) >= squared(start, node.tile)),
        "no nearer same-kind source"
    );
    assert!(w.nearest_resource(start, kind, 10, None).is_none());
    let center = TileCoord::new(castle_rect.min.x + 2, castle_rect.min.y + 2);
    assert!(squared(center, node.tile) < (crate::VASSAL_RULES.radius_tiles as u64).pow(2));
    let castle = w
        .insert_building(PlayerId(0), BuildingKind::Castle, castle_rect, true)
        .unwrap();
    if with_camp {
        w.insert_building(PlayerId(0), BuildingKind::MiningCamp, camp_rect, true)
            .unwrap();
    }
    let p = w.economy.players.get_mut(&PlayerId(0)).unwrap();
    p.age = Age::Castle;
    reserves(
        &mut w,
        1_000_000,
        1_000_000,
        if kind == ResourceKind::Gold {
            0
        } else {
            1_000_000
        },
        if kind == ResourceKind::Stone {
            0
        } else {
            1_000_000
        },
    );
    let worker = w.spawn_villager(PlayerId(0), position(start)).unwrap();
    let exit = (castle_rect.min.y - 1..=castle_rect.max.y)
        .flat_map(|y| {
            (castle_rect.min.x - 1..=castle_rect.max.x).map(move |x| TileCoord::new(x, y))
        })
        .find(|tile| w.tile_free(*tile))
        .unwrap();
    let vassal = w
        .spawn_kind(PlayerId(0), UnitKind::Vassal, position(exit))
        .unwrap();
    w.bind_vassal(vassal, castle, castle_rect);
    w.issue_garrison(PlayerId(0), vassal, castle).unwrap();
    w.advance_garrisons(&mut Vec::new());
    assert!(w.vassal_status(vassal).is_some());
    (w, worker, vassal, node)
}

fn bounded(work: crate::VassalWork) {
    // Extend with explicit finder block/node visits and a many-domain attack
    // when those real counters exist; no future metrics API is invented here.
    let rules = crate::VASSAL_RULES;
    assert!(work.domains <= rules.domains_per_pass);
    assert!(work.unit_visits <= work.domains * rules.unit_visits_per_domain);
    assert!(work.building_visits <= work.domains * rules.building_visits_per_domain);
    assert!(work.orders <= work.domains * rules.orders_per_domain);
    assert!(work.resource_searches <= work.domains * rules.resource_searches_per_domain);
    assert!(work.site_checks <= work.domains * rules.site_checks_per_domain);
}

fn discovers_and_deposits(kind: ResourceKind) {
    let (mut a, a_worker, a_vassal, _) = distant_ore(kind, true);
    let (mut b, b_worker, b_vassal, _) = distant_ore(kind, true);
    let mut mining = false;
    for tick in 0..10_000 {
        a.advance();
        b.advance();
        assert_eq!(
            a.canonical_hash(),
            b.canonical_hash(),
            "distant ore replay tick {tick}"
        );
        assert_eq!(a.vassal_work(), b.vassal_work());
        bounded(a.vassal_work());
        bounded(b.vassal_work());
        mining |=
            a.gathering_kind(a_worker) == Some(kind) && b.gathering_kind(b_worker) == Some(kind);
        if mining
            && resource_stock(&a, kind) >= u32::from(VILLAGE_RULES.villager_capacity)
            && resource_stock(&b, kind) >= u32::from(VILLAGE_RULES.villager_capacity)
        {
            assert_eq!(a.vassal_controller(a_worker), Some(a_vassal));
            assert_eq!(b.vassal_controller(b_worker), Some(b_vassal));
            return;
        }
    }
    panic!(
        "visible in-domain {kind:?} beyond ten tiles must produce actual deposits; stocks {}/{}",
        resource_stock(&a, kind),
        resource_stock(&b, kind)
    );
}

#[test]
fn visible_distant_gold_is_discovered_with_bounded_work_and_replays() {
    discovers_and_deposits(ResourceKind::Gold);
}

#[test]
fn visible_distant_stone_is_discovered_with_bounded_work_and_replays() {
    discovers_and_deposits(ResourceKind::Stone);
}

/// Far ore on the plot needs its own camp: the Vassal builds one beside the
/// ore (the Castle is far away), and its miners then deposit real income.
fn far_ore_without_a_camp_gets_a_camp_beside_it(kind: ResourceKind) {
    let (mut w, worker, vassal, node) = distant_ore(kind, false);
    assert_eq!(
        w.player_buildings_of(PlayerId(0), BuildingKind::MiningCamp)
            .count(),
        0
    );
    let (mut camp_beside_ore, mut mining) = (false, false);
    for _ in 0..10_000 {
        w.advance();
        bounded(w.vassal_work());
        camp_beside_ore |= w
            .player_buildings_of(PlayerId(0), BuildingKind::MiningCamp)
            .any(|b| squared(b.footprint.min, node.tile) <= 8 * 8);
        mining |=
            w.vassal_controller(worker) == Some(vassal) && w.gathering_kind(worker) == Some(kind);
        if camp_beside_ore
            && mining
            && resource_stock(&w, kind) >= u32::from(VILLAGE_RULES.villager_capacity)
        {
            return;
        }
    }
    panic!(
        "far {kind:?} must get a camp beside it (built: {camp_beside_ore}) and be mined (mining: {mining})"
    );
}

#[test]
fn far_gold_on_the_plot_gets_a_mining_camp_beside_it_and_is_mined() {
    far_ore_without_a_camp_gets_a_camp_beside_it(ResourceKind::Gold);
}

//! Kingdom shortages, observed deposit income and bounded distant discovery.
use super::*;
use std::collections::BTreeSet;
mod discovery;
mod income;
mod manual;
mod rebalancing;
mod showcase;

fn reserves(w: &mut GameWorld, food: u32, wood: u32, gold: u32, stone: u32) {
    w.economy.players.get_mut(&PlayerId(0)).unwrap().stockpile = Stockpile {
        food,
        wood,
        gold,
        stone,
    };
}

fn position(tile: TileCoord) -> WorldPosition {
    WorldPosition::from_tile_center(tile).unwrap()
}

fn place_near(w: &mut GameWorld, kind: BuildingKind, anchor: TileCoord) -> EntityId {
    for radius in 0_i32..32 {
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx.abs().max(dy.abs()) != radius {
                    continue;
                }
                let tile = TileCoord::new(anchor.x + dx, anchor.y + dy);
                if w.validate_building_placement(kind, tile).is_ok() {
                    return w
                        .insert_building(
                            PlayerId(0),
                            kind,
                            aoe_core::TileRect::from_xywh(
                                tile.x,
                                tile.y,
                                kind.rules().side,
                                kind.rules().depth,
                            ),
                            true,
                        )
                        .unwrap();
                }
            }
        }
    }
    panic!("fixture needs a clear {kind:?} site near {anchor:?}");
}

fn workers_near(w: &mut GameWorld, near: TileCoord, count: usize) -> Vec<EntityId> {
    let mut workers = Vec::new();
    for radius in 1_i32..10 {
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx.abs().max(dy.abs()) != radius {
                    continue;
                }
                let tile = TileCoord::new(near.x + dx, near.y + dy);
                if w.tile_free(tile) {
                    workers.push(w.spawn_villager(PlayerId(0), position(tile)).unwrap());
                    if workers.len() == count {
                        return workers;
                    }
                }
            }
        }
    }
    panic!("fixture needs {count} free work spots near {near:?}");
}

fn assigned(w: &GameWorld, vassal: EntityId, workers: &[EntityId], kind: ResourceKind) -> usize {
    workers
        .iter()
        .filter(|id| {
            w.vassal_controller(**id) == Some(vassal) && w.gathering_kind(**id) == Some(kind)
        })
        .count()
}

fn wood_realm(count: usize) -> (GameWorld, EntityId, Vec<EntityId>, ResourceNode) {
    let (mut w, node, _, vassal) = super::resources::wood_domain(1);
    w.set_fog_disabled(true);
    let wolves: Vec<_> = w
        .animals()
        .filter(|a| a.kind == crate::AnimalKind::Wolf)
        .map(|a| a.id)
        .collect();
    for wolf in wolves {
        w.strike_animal(wolf, vassal, u16::MAX);
    }
    let old: Vec<_> = w
        .units()
        .filter(|u| u.kind == UnitKind::Villager)
        .map(|u| u.id)
        .collect();
    for id in old {
        w.remove_unit(id);
    }
    let castle = w.vassal_castle(vassal).unwrap();
    let anchor = w.building(castle).unwrap().footprint.min;
    for _ in 0..12 {
        place_near(&mut w, BuildingKind::House, TileCoord::new(15, 15));
    }
    place_near(&mut w, BuildingKind::LumberCamp, node.tile);
    place_near(
        &mut w,
        BuildingKind::Mill,
        TileCoord::new(anchor.x + 7, anchor.y + 7),
    );
    let workers = workers_near(&mut w, node.tile, count);
    assert!(
        w.nearest_resource(node.tile, ResourceKind::Wood, 10, Some(node.id))
            .is_some()
    );
    advance(&mut w, 2);
    assert!(w.vassal_status(vassal).is_some());
    (w, vassal, workers, node)
}

#[test]
fn kingdom_wood_shortage_puts_nearly_all_available_workers_on_wood() {
    let (mut w, vassal, workers, _) = wood_realm(20);
    // Food is a real competing job: the completed Mill and 1,000 wood permit
    // ordinary farms. This is not a forced wood-only assignment fixture.
    assert!(
        w.player_buildings_of(PlayerId(0), BuildingKind::Mill)
            .any(|b| b.is_complete())
    );
    reserves(&mut w, 30_000, 1_000, 30_000, 30_000);
    advance(&mut w, 800);
    let wood = assigned(&w, vassal, &workers, ResourceKind::Wood);
    assert!(wood >= 18, "wood shortage received only {wood}/20 workers");
    let targets: BTreeSet<_> = workers
        .iter()
        .filter_map(|id| w.gather_target(*id))
        .collect();
    assert!(
        targets.len() >= 2,
        "exercise more than one tree reservation"
    );
}

fn resource_stock(w: &GameWorld, kind: ResourceKind) -> u32 {
    let stock = w.player_economy(PlayerId(0)).unwrap().stockpile;
    match kind {
        ResourceKind::Food => stock.food,
        ResourceKind::Wood => stock.wood,
        ResourceKind::Gold => stock.gold,
        ResourceKind::Stone => stock.stone,
    }
}

fn observed_deposits(w: &mut GameWorld, ticks: u32, kind: ResourceKind) -> u32 {
    // These fixtures perform no trades, refunds or stock resets in this
    // interval; positive stock increments therefore witness real deposits.
    let mut before = resource_stock(w, kind);
    let mut observed = 0;
    for _ in 0..ticks {
        w.advance();
        let next = resource_stock(w, kind);
        observed += next.saturating_sub(before);
        before = next;
    }
    observed
}

fn assign_farms(w: &mut GameWorld, workers: &[EntityId], anchor: TileCoord) {
    for worker in workers {
        let farm = place_near(w, BuildingKind::Farm, anchor);
        w.issue_farm(PlayerId(0), *worker, farm).unwrap();
    }
}

fn make_farmers(w: &mut GameWorld, workers: &[EntityId], anchor: TileCoord) -> u32 {
    assign_farms(w, workers, anchor);
    advance(w, 3_000);
    let deposited = observed_deposits(w, 1_000, ResourceKind::Food);
    assert!(
        workers
            .iter()
            .all(|id| w.gathering_kind(*id) == Some(ResourceKind::Food))
    );
    deposited
}

fn distant_manual_destination(w: &GameWorld, from: WorldPosition) -> WorldPosition {
    let mut corners = [
        TileCoord::new(16, 16),
        TileCoord::new(230, 16),
        TileCoord::new(16, 230),
        TileCoord::new(230, 230),
    ];
    let origin = from.tile_floor();
    corners.sort_by_key(|t| std::cmp::Reverse(origin.x.abs_diff(t.x) + origin.y.abs_diff(t.y)));
    for corner in corners {
        for dy in -5..=5 {
            for dx in -5..=5 {
                let tile = TileCoord::new(corner.x + dx, corner.y + dy);
                if w.tile_free(tile) {
                    return position(tile);
                }
            }
        }
    }
    panic!("fixture needs a distant manual move destination");
}

fn far_mining_camp(kind: ResourceKind) {
    let (mut w, node, worker) = resource_world(kind);
    w.set_fog_disabled(true);
    // Zero wood makes a closer camp unaffordable. Remove competing local
    // wood through the normal depletion API so that this does not turn into
    // a different, equally valid kingdom-wood-shortage decision.
    let origin = w.unit(worker).unwrap().position.tile_floor();
    for _ in 0..512 {
        let Some(tree) = w.nearest_resource(origin, ResourceKind::Wood, 10, None) else {
            break;
        };
        w.deplete_resource(tree.id, tree.initial_amount).unwrap();
    }
    assert!(
        w.nearest_resource(origin, ResourceKind::Wood, 10, None)
            .is_none()
    );
    let castle = place_near(
        &mut w,
        BuildingKind::Castle,
        TileCoord::new((node.tile.x + 20).min(235), (node.tile.y + 20).min(235)),
    );
    let footprint = w.building(castle).unwrap().footprint;
    let camp = place_near(&mut w, BuildingKind::MiningCamp, footprint.min);
    let rect = w.building(camp).unwrap().footprint;
    let dx = node.tile.x - node.tile.x.clamp(rect.min.x - 1, rect.max.x);
    let dy = node.tile.y - node.tile.y.clamp(rect.min.y - 1, rect.max.y);
    assert!(dx * dx + dy * dy > 64, "fixture camp must be far from ore");
    let exit = (footprint.min.y - 1..=footprint.max.y)
        .flat_map(|y| (footprint.min.x - 1..=footprint.max.x).map(move |x| TileCoord::new(x, y)))
        .find(|tile| w.tile_free(*tile))
        .unwrap();
    let vassal = w
        .spawn_kind(PlayerId(0), UnitKind::Vassal, position(exit))
        .unwrap();
    w.bind_vassal(vassal, castle, footprint);
    w.issue_garrison(PlayerId(0), vassal, castle).unwrap();
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
    let mut targeted = false;
    for _ in 0..4_000 {
        w.advance();
        targeted |= w.vassal_controller(worker) == Some(vassal)
            && w.gathering_kind(worker) == Some(kind)
            && w.gather_target(worker) == Some(node.id);
        let stock = w.player_economy(PlayerId(0)).unwrap().stockpile;
        let amount = if kind == ResourceKind::Gold {
            stock.gold
        } else {
            stock.stone
        };
        if targeted && amount >= u32::from(VILLAGE_RULES.villager_capacity) {
            assert_eq!(
                w.player_buildings_of(PlayerId(0), BuildingKind::MiningCamp)
                    .count(),
                1
            );
            return;
        }
    }
    panic!("{kind:?} worker must use a distant camp when a new camp is unaffordable");
}

#[test]
fn gold_worker_uses_a_far_mining_camp_and_deposits_income() {
    far_mining_camp(ResourceKind::Gold);
}

#[test]
fn stone_worker_uses_a_far_mining_camp_and_deposits_income() {
    far_mining_camp(ResourceKind::Stone);
}

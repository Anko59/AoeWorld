use super::*;
use crate::VASSAL_RULES;

fn wood_domain(workers: usize) -> (GameWorld, ResourceNode, EntityId, EntityId) {
    let (mut w, node, worker) = resource_world(ResourceKind::Wood);
    let near = TileCoord::new(node.tile.x + 14, node.tile.y + 14);
    w.initialize_village(PlayerId(0), near).unwrap();
    let tc = w
        .player_buildings_of(PlayerId(0), BuildingKind::TownCenter)
        .next()
        .unwrap()
        .id;
    let anchor = w.building(tc).unwrap().footprint.min;
    let mut site = None;
    for radius in 6..=40 {
        for (dx, dy) in [(radius, 0), (0, radius), (-radius, 0), (0, -radius)] {
            let at = TileCoord::new(anchor.x + dx, anchor.y + dy);
            if w.validate_building_placement(BuildingKind::Castle, at)
                .is_ok()
            {
                site = Some(at);
                break;
            }
        }
        if site.is_some() {
            break;
        }
    }
    let at = site.expect("local clear Castle site");
    let footprint = aoe_core::TileRect::from_xywh(at.x, at.y, 4, 4);
    let castle = w
        .insert_building(PlayerId(0), BuildingKind::Castle, footprint, true)
        .unwrap();
    let p = w.economy.players.get_mut(&PlayerId(0)).unwrap();
    p.age = Age::Castle;
    p.stockpile = Stockpile {
        food: 10_000,
        wood: 1_000,
        gold: 10_000,
        stone: 10_000,
    };
    // With no drop-off left, the manager must build one through normal orders.
    w.destroy_building(tc);
    let exit = (at.y - 1..=at.y + 4)
        .flat_map(|y| (at.x - 1..=at.x + 4).map(move |x| TileCoord::new(x, y)))
        .find(|t| w.tile_free(*t))
        .unwrap();
    let id = w
        .spawn_kind(
            PlayerId(0),
            UnitKind::Vassal,
            WorldPosition::from_tile_center(exit).unwrap(),
        )
        .unwrap();
    w.bind_vassal(id, castle, footprint);
    w.issue_garrison(PlayerId(0), id, castle).unwrap();
    for _ in 1..workers {
        w.spawn_villager(PlayerId(0), w.unit(worker).unwrap().position)
            .unwrap();
    }
    (w, node, worker, id)
}

#[test]
fn known_wood_is_assigned_a_lumber_camp_is_built_and_loads_are_deposited() {
    let (mut w, _, worker, id) = wood_domain(1);
    advance(&mut w, VASSAL_RULES.period_ticks as u32 * 2);
    assert_eq!(w.vassal_controller(worker), Some(id));
    assert!(
        w.player_buildings_of(PlayerId(0), BuildingKind::LumberCamp)
            .next()
            .is_some()
    );
    advance(&mut w, 2_000);
    assert!(
        w.player_buildings_of(PlayerId(0), BuildingKind::LumberCamp)
            .any(|b| b.is_complete())
    );
    assert!(w.units().any(|u| w.vassal_controller(u.id) == Some(id)
        && w.gathering_kind(u.id) == Some(ResourceKind::Wood)));
    assert!(
        w.player_economy(PlayerId(0)).unwrap().stockpile.wood > 900,
        "normal gathering deposited wood after paying for the camp"
    );
}

// The bundle combines automatic Vassal jobs with eco2 reservations and perf3
// walking proposals. Managed orders must maintain the ordinary target index,
// including workers approaching a tree or returning with a load.
#[test]
fn managed_lumberjacks_share_the_normal_reservation_path() {
    use crate::game_world::economy::WorkTarget;
    use std::collections::BTreeMap;
    let (mut w, _, _, vassal) = wood_domain(8);
    w.economy.players.get_mut(&PlayerId(0)).unwrap().stockpile = Stockpile {
        food: 4_000,
        wood: 1_000,
        gold: 4_000,
        stone: 4_000,
    };
    let mut managed_peak = 0;
    for _ in 0..2_400 {
        w.advance();
        let mut targets = BTreeMap::<u64, usize>::new();
        let mut managed = 0;
        for unit in w.units() {
            if w.gathering_kind(unit.id) == Some(ResourceKind::Wood)
                && let Some(target) = w.gather_target(unit.id)
            {
                *targets.entry(target).or_default() += 1;
                managed += usize::from(w.vassal_controller(unit.id) == Some(vassal));
            }
        }
        for (target, count) in targets {
            assert!(count <= 12, "tree {target} has {count} assigned workers");
            assert_eq!(
                w.economy.gatherers.count_target(WorkTarget::Node(target)),
                count
            );
        }
        managed_peak = managed_peak.max(managed);
    }
    assert!(
        managed_peak >= 3,
        "multiple managed trees must be exercised"
    );
}

use super::*;
use std::collections::BTreeMap;

// These crops have clear ordinary delivery routes. A watchdog-abandoned job
// is a different boundary from productive work interrupted by a stock change.
fn productive_farmers() -> (GameWorld, EntityId, Vec<EntityId>, ResourceNode) {
    let (mut w, _) = GameWorld::steward_stability_demo(aoe_core::Seed(7)).unwrap();
    // Use normal game speed for all deadlines in these regressions.
    w.config.tick_hz = 20;
    let steward = w.units().find(|u| u.kind == UnitKind::Steward).unwrap().id;
    let tc = w
        .buildings()
        .find(|b| b.kind == BuildingKind::TownCenter)
        .unwrap()
        .id;
    let workers: Vec<_> = w
        .units()
        .filter(|u| u.kind == UnitKind::Villager)
        .map(|u| u.id)
        .collect();
    let from = w.building(tc).unwrap().footprint.min;
    let node = w
        .nearest_resource(from, ResourceKind::Wood, 100, None)
        .unwrap();
    w.issue_garrison(PlayerId(0), steward, tc).unwrap();
    for _ in 0..800 {
        w.advance();
        if workers
            .iter()
            .all(|id| w.steward_controller(*id) == Some(steward))
        {
            break;
        }
    }
    assert_eq!(assigned(&w, steward, &workers, ResourceKind::Food), 20);
    (w, steward, workers, node)
}

#[test]
fn moderate_shortage_preserves_farmers_and_assigns_new_idle_workers() {
    let (mut w, steward, workers, node) = productive_farmers();
    // Food/45 = 40 and Wood/30 = 33.3: a real moderate imbalance,
    // but no emergency that should interrupt the entire workforce at once.
    reserves(&mut w, 1_800, 1_000, 30_000, 30_000);
    let idle = workers_near(&mut w, node.tile, 2);
    advance(&mut w, 40);
    assert_eq!(
        assigned(&w, steward, &workers, ResourceKind::Wood),
        0,
        "productive farmers keep their jobs under a moderate shortage"
    );
    assert!(assigned(&w, steward, &workers, ResourceKind::Food) >= 18);
    assert_eq!(
        assigned(&w, steward, &idle, ResourceKind::Wood),
        2,
        "new idle workers still respond within two game seconds"
    );
}

fn carried_food(w: &GameWorld) -> BTreeMap<EntityId, u16> {
    w.units()
        .filter(|u| u.player == PlayerId(0))
        .filter_map(|u| {
            u.carried
                .filter(|load| load.kind == ResourceKind::Food)
                .map(|load| (u.id, load.amount))
        })
        .collect()
}

fn at_food_dropoff(w: &GameWorld, id: EntityId) -> bool {
    let at = w.unit(id).unwrap().position.tile_floor();
    w.player_buildings(PlayerId(0)).any(|b| {
        b.is_complete()
            && b.kind.accepts(ResourceKind::Food)
            && !b.footprint.contains(at)
            && at.x >= b.footprint.min.x - 1
            && at.x <= b.footprint.max.x
            && at.y >= b.footprint.min.y - 1
            && at.y <= b.footprint.max.y
    })
}

#[test]
fn established_farmers_keep_plots_and_deliver_loads_when_kingdom_needs_change() {
    let (mut w, steward, workers, _) = productive_farmers();
    let food = observed_deposits(&mut w, 1_000, ResourceKind::Food);
    assert!(
        food >= 200,
        "established farming must actually deposit food"
    );
    let manual = workers[0];
    let destination = distant_manual_destination(&w, w.unit(manual).unwrap().position);
    w.issue_move(manual, destination).unwrap();
    let eligible = &workers[1..];
    let mut loaded = None;
    for _ in 0..1_000 {
        loaded = eligible.iter().find_map(|id| {
            let load = w.unit(*id)?.carried?;
            (load.kind == ResourceKind::Food && load.amount > 0).then_some((*id, load.amount))
        });
        if loaded.is_some() {
            break;
        }
        w.advance();
    }
    let (loaded_worker, load) = loaded.expect("normal farming must carry food");
    assert_eq!(w.steward_controller(loaded_worker), Some(steward));
    reserves(&mut w, 30_000, 1_000, 30_000, 30_000);
    let targets: BTreeMap<_, _> = eligible
        .iter()
        .map(|id| (*id, w.gather_tile(*id)))
        .collect();
    let mut own_deposit = 0_u32;
    for _ in 0..800 {
        let before = carried_food(&w);
        let food_before = resource_stock(&w, ResourceKind::Food);
        w.advance();
        let after = carried_food(&w);
        let released: u32 = before
            .iter()
            .map(|(id, amount)| {
                u32::from(amount.saturating_sub(after.get(id).copied().unwrap_or(0)))
            })
            .sum();
        // Other farmers' deposits cannot hide this worker losing its load.
        assert!(resource_stock(&w, ResourceKind::Food).saturating_sub(food_before) >= released);
        let own = before
            .get(&loaded_worker)
            .copied()
            .unwrap_or(0)
            .saturating_sub(after.get(&loaded_worker).copied().unwrap_or(0));
        if own > 0 {
            assert!(
                at_food_dropoff(&w, loaded_worker),
                "own Food clears only at normal drop-off contact"
            );
            own_deposit += u32::from(own);
        }
        for worker in eligible {
            if let Some(job) = w.economy.builders.get(worker) {
                assert!(matches!(
                    w.building(job.building).unwrap().kind,
                    BuildingKind::Mill | BuildingKind::House | BuildingKind::Dock
                ));
                assert!(w.steward_has_held_job(*worker));
                continue;
            }
            if w.gathering_kind(*worker).is_none() {
                continue;
            }
            assert_eq!(w.gathering_kind(*worker), Some(ResourceKind::Food));
            assert_eq!(
                w.gather_tile(*worker),
                targets[worker],
                "productive plots are retained"
            );
        }
        assert_eq!(w.steward_controller(manual), None);
        assert_eq!(
            w.movement_order(manual).map(|o| o.destination),
            Some(destination)
        );
    }
    assert_eq!(
        eligible
            .iter()
            .filter(|id| {
                (w.gathering_kind(**id) == Some(ResourceKind::Food)
                    && w.gather_tile(**id) == targets[id])
                    || (w.economy.builders.contains_key(id) && w.steward_has_held_job(**id))
            })
            .count(),
        19
    );
    assert!(
        own_deposit >= u32::from(load),
        "the loaded farmer still delivers its own Food"
    );
}

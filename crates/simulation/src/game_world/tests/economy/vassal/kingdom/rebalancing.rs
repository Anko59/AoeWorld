use super::*;
use std::collections::BTreeMap;

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
fn established_farmers_respond_to_changed_kingdom_needs_without_losing_loads_or_manual_orders() {
    let (mut w, vassal, workers, _) = wood_realm(20);
    reserves(&mut w, 30_000, 30_000, 30_000, 30_000);
    let castle = w.vassal_castle(vassal).unwrap();
    let anchor = w.building(castle).unwrap().footprint.min;
    let food = make_farmers(
        &mut w,
        &workers,
        TileCoord::new(anchor.x + 10, anchor.y + 10),
    );
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
    reserves(&mut w, 30_000, 1_000, 30_000, 30_000);
    let mut switched_loaded_worker = false;
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
        if w.gathering_kind(loaded_worker) == Some(ResourceKind::Wood) {
            switched_loaded_worker = true;
            assert!(own_deposit >= u32::from(load));
            assert!(
                w.player_economy(PlayerId(0)).unwrap().stockpile.food >= 30_000 + u32::from(load)
            );
        }
        assert_eq!(w.vassal_controller(manual), None);
        assert_eq!(
            w.movement_order(manual).map(|o| o.destination),
            Some(destination)
        );
    }
    let wood = assigned(&w, vassal, eligible, ResourceKind::Wood);
    assert!(
        wood >= 18,
        "established farmers did not respond: {wood}/19 on wood"
    );
    assert!(
        switched_loaded_worker,
        "a loaded farmer must deposit, then respond too"
    );
}

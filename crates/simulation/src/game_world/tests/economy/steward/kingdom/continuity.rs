use super::*;

fn keeps_productive_job(kind: ResourceKind) {
    let (mut w, steward, workers, _) = wood_realm(20);
    if kind == ResourceKind::Food {
        reserves(&mut w, 1_000, 30_000, 30_000, 30_000);
        let center = w.steward_town_center(steward).unwrap();
        let anchor = w.building(center).unwrap().footprint.min;
        assign_farms(&mut w, &workers, TileCoord::new(anchor.x + 7, anchor.y + 7));
    } else {
        reserves(&mut w, 30_000, 1_000, 30_000, 30_000);
    }
    let mut working = None;
    for _ in 0..3_000 {
        w.advance();
        working = workers.iter().copied().find(|id| {
            w.steward_controller(*id) == Some(steward)
                && w.gathering_kind(*id) == Some(kind)
                && matches!(
                    w.unit(*id).unwrap().task,
                    UnitTask::Farming | UnitTask::Gathering
                )
        });
        if working.is_some() {
            break;
        }
    }
    let worker = working.expect("a real productive job must have started");
    let tile = w.gather_tile(worker).unwrap();
    let farm = w
        .buildings()
        .find(|b| b.kind == BuildingKind::Farm && b.footprint.contains(tile))
        .map(|b| b.id);
    let node = w.gather_target(worker);
    // Both moderate and urgent kingdom needs may choose idle workers, but
    // neither interrupts a productive Farm or a still-yielding woodline.
    for scarce in [1_000, 0] {
        if kind == ResourceKind::Food {
            reserves(&mut w, 30_000, scarce, 30_000, 30_000);
        } else {
            reserves(&mut w, scarce, 30_000, 30_000, 30_000);
        }
        for _ in 0..200 {
            w.advance();
            assert_eq!(
                (
                    w.gathering_kind(worker),
                    w.gather_tile(worker),
                    w.gather_target(worker)
                ),
                (Some(kind), Some(tile), node),
                "kingdom reserve changes must not abandon a productive job"
            );
            let remaining = farm.map_or_else(
                || node.and_then(|node| w.resource_remaining(node)),
                |farm| w.building(farm).unwrap().food_remaining,
            );
            assert!(
                remaining.is_some_and(|left| left > 0),
                "exercise an unfinished job"
            );
        }
    }
}

#[test]
fn productive_farm_survives_moderate_and_urgent_stock_changes() {
    keeps_productive_job(ResourceKind::Food);
}

#[test]
fn productive_woodline_survives_moderate_and_urgent_stock_changes() {
    keeps_productive_job(ResourceKind::Wood);
}

#[test]
fn a_spent_tree_continues_its_productive_local_woodline() {
    let (mut w, steward, workers, _) = wood_realm(1);
    let worker = workers[0];
    reserves(&mut w, 30_000, 1_000, 30_000, 30_000);
    for _ in 0..3_000 {
        w.advance();
        if w.steward_controller(worker) == Some(steward)
            && w.unit(worker)
                .unwrap()
                .carried
                .is_some_and(|c| c.kind == ResourceKind::Wood)
        {
            break;
        }
    }
    let load = w.unit(worker).unwrap().carried.expect("real Wood delivery");
    let old = w.gather_target(worker).unwrap();
    let tile = w.gather_tile(worker).unwrap();
    assert!(
        w.nearest_resource(tile, ResourceKind::Wood, 10, Some(old))
            .is_some()
    );
    w.deplete_resource(old, w.resource_remaining(old).unwrap())
        .unwrap();
    reserves(&mut w, 0, 30_000, 30_000, 30_000);
    let mut replacement = None;
    for _ in 0..1_200 {
        w.advance();
        if let Some(next) = w.gather_target(worker).filter(|id| *id != old) {
            assert_eq!(w.gathering_kind(worker), Some(ResourceKind::Wood));
            assert!(
                w.player_economy(PlayerId(0)).unwrap().stockpile.wood
                    >= 30_000 + u32::from(load.amount)
            );
            replacement = Some(next);
            break;
        }
    }
    let replacement = replacement.expect("continue on a live local tree");
    let next = w.gather_tile(worker).unwrap();
    let radius = u64::from(VILLAGE_RULES.replacement_radius_tiles as u32);
    assert!(
        u64::from(tile.x.abs_diff(next.x)).pow(2) + u64::from(tile.y.abs_diff(next.y)).pow(2)
            <= radius * radius
    );
    let mut repaired = false;
    let mut resumed = false;
    let mut idle = 0;
    for _ in 0..2_000 {
        w.advance();
        if let Some(job) = w.economy.builders.get(&worker) {
            assert!(matches!(
                w.building(job.building).unwrap().kind,
                BuildingKind::LumberCamp | BuildingKind::Dock
            ));
            assert!(w.steward_has_held_job(worker));
            repaired = true;
        } else if w.gathering_kind(worker).is_none() {
            idle += 1;
            assert!(idle <= 2 * crate::STEWARD_RULES.period_ticks);
        } else {
            assert_eq!(w.gather_target(worker), Some(replacement));
            assert_eq!(w.gathering_kind(worker), Some(ResourceKind::Wood));
            resumed |= repaired;
        }
        assert!(
            w.resource_remaining(replacement)
                .is_some_and(|left| left > 0)
        );
    }
    assert!(
        repaired && resumed,
        "support construction resumes the same productive woodline"
    );
}

#[test]
fn exhausted_managed_woodline_delivers_then_selects_food() {
    let (mut w, steward, workers, _) = wood_realm(1);
    let worker = workers[0];
    reserves(&mut w, 30_000, 1_000, 30_000, 30_000);
    let mut load = None;
    for _ in 0..3_000 {
        w.advance();
        load = w
            .unit(worker)
            .unwrap()
            .carried
            .filter(|c| c.kind == ResourceKind::Wood);
        if load.is_some() && w.steward_controller(worker) == Some(steward) {
            break;
        }
    }
    let load = load.expect("ordinary logging must carry Wood");
    let node = w.gather_target(worker).unwrap();
    let tile = w.gather_tile(worker).unwrap();
    for _ in 0..512 {
        let Some(tree) = w.nearest_resource(tile, ResourceKind::Wood, 10, None) else {
            break;
        };
        w.deplete_resource(tree.id, w.resource_remaining(tree.id).unwrap())
            .unwrap();
    }
    assert!(
        w.nearest_resource(tile, ResourceKind::Wood, 10, None)
            .is_none(),
        "the local woodline really ends"
    );
    assert_eq!(w.resource_remaining(node), Some(0));
    reserves(&mut w, 0, 30_000, 30_000, 30_000);
    for _ in 0..1_200 {
        w.advance();
        let food_job = w.gathering_kind(worker) == Some(ResourceKind::Food)
            || w.economy
                .builders
                .get(&worker)
                .is_some_and(|task| w.building(task.building).unwrap().kind == BuildingKind::Farm);
        if food_job {
            assert!(
                w.player_economy(PlayerId(0)).unwrap().stockpile.wood
                    >= 30_000 + u32::from(load.amount)
            );
            assert!(
                w.unit(worker)
                    .unwrap()
                    .carried
                    .is_none_or(|c| c.kind != ResourceKind::Wood)
            );
            return;
        }
    }
    panic!("depletion must make the logger available for needed Food work");
}

#[test]
fn an_owner_move_clears_the_job_held_during_support_construction() {
    let (mut w, steward, workers, _) = wood_realm(1);
    let worker = workers[0];
    reserves(&mut w, 30_000, 1_000, 30_000, 30_000);
    for _ in 0..3_000 {
        w.advance();
        if w.gathering_kind(worker) == Some(ResourceKind::Wood)
            && w.unit(worker).unwrap().task == UnitTask::Gathering
        {
            break;
        }
    }
    assert_eq!(w.unit(worker).unwrap().task, UnitTask::Gathering);
    let camps: Vec<_> = w
        .player_buildings_of(PlayerId(0), BuildingKind::LumberCamp)
        .map(|b| b.id)
        .collect();
    for camp in camps {
        w.remove_building(camp);
    }
    reserves(&mut w, 0, 30_000, 30_000, 30_000);
    for _ in 0..2_000 {
        w.advance();
        if w.economy.builders.contains_key(&worker) && w.steward_has_held_job(worker) {
            let destination = distant_manual_destination(&w, w.unit(worker).unwrap().position);
            w.issue_move(worker, destination).unwrap();
            for _ in 0..200 {
                w.advance();
                assert!(!w.steward_has_held_job(worker));
                assert_eq!(w.steward_controller(worker), None);
                assert_eq!(
                    w.movement_order(worker).map(|o| o.destination),
                    Some(destination)
                );
            }
            return;
        }
    }
    panic!("expected a managed logger borrowed for support construction by {steward:?}");
}

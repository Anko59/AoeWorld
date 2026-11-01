use super::*;
mod events;
mod history;

struct RemoteIncome {
    w: GameWorld,
    local: EntityId,
    local_workers: Vec<EntityId>,
    remote: EntityId,
    remote_workers: Vec<EntityId>,
    deposits: u32,
    warmup_deposits: u32,
}

fn remote_wood(w: &GameWorld, local: TileCoord) -> ResourceNode {
    for y in 1..15 {
        for x in 1..15 {
            let origin = TileCoord::new(x * 16, y * 16);
            let Some(node) = w.nearest_resource(origin, ResourceKind::Wood, 10, None) else {
                continue;
            };
            let dx = i64::from(node.tile.x - local.x);
            let dy = i64::from(node.tile.y - local.y);
            if dx * dx + dy * dy > 190 * 190
                && node.tile.x < 225
                && node.tile.y < 225
                && w.nearest_resource(node.tile, ResourceKind::Wood, 10, Some(node.id))
                    .is_some()
            {
                return node;
            }
        }
    }
    panic!("fixture needs a separate wooded domain");
}

fn stalled_food(
    w: &mut GameWorld,
    remote: EntityId,
    castle: EntityId,
    workers: &[EntityId],
) -> (u32, u32) {
    let anchor = w.building(castle).unwrap().footprint.min;
    let local_mill = w
        .player_buildings_of(PlayerId(0), BuildingKind::Mill)
        .next()
        .unwrap()
        .clone();
    reserves(w, 0, 30_000, 30_000, 30_000);
    w.issue_garrison(PlayerId(0), remote, castle).unwrap();
    let earned = make_farmers(w, workers, anchor);
    assert!(
        earned >= 200,
        "stalled group must have old real deposits to expire"
    );
    w.issue_ungarrison(PlayerId(0), castle, Some(remote))
        .unwrap();
    // Ordinary farm repair may have added more Mills during productive
    // warmup. Remove every food-accepting drop-off, including foundations.
    let dropoffs: Vec<_> = w
        .player_buildings(PlayerId(0))
        .filter(|b| b.kind.accepts(ResourceKind::Food))
        .map(|b| b.id)
        .collect();
    for id in dropoffs {
        w.remove_building(id);
    }
    assert!(
        w.nearest_dropoff(
            PlayerId(0),
            w.unit(workers[0]).unwrap().position.tile_floor(),
            ResourceKind::Food
        )
        .is_none()
    );
    let food = resource_stock(w, ResourceKind::Food);
    reserves(w, food, 0, 30_000, 30_000);
    // While neither domain manages them, the real farmers fill their loads
    // without a drop-off. Preserve their ordinary Food jobs before readoption.
    advance(w, 2_000);
    assert_eq!(resource_stock(w, ResourceKind::Food), food);
    assert!(
        workers
            .iter()
            .all(|id| w.unit(*id).unwrap().carried.is_some_and(|load| {
                load.kind == ResourceKind::Food && load.amount >= VILLAGE_RULES.villager_capacity
            })),
        "stalled farmers must retain full real food loads"
    );
    w.issue_garrison(PlayerId(0), remote, castle).unwrap();
    advance(w, 20);
    assert_eq!(
        assigned(w, remote, workers, ResourceKind::Food),
        workers.len()
    );
    w.validate_building_placement(BuildingKind::Mill, local_mill.footprint.min)
        .unwrap();
    w.insert_building(PlayerId(0), BuildingKind::Mill, local_mill.footprint, true)
        .unwrap();
    let rect = w.building(castle).unwrap().footprint;
    let center = TileCoord::new(rect.min.x + 2, rect.min.y + 2);
    for dropoff in w
        .player_buildings(PlayerId(0))
        .filter(|b| b.kind.accepts(ResourceKind::Food))
    {
        let dx = i64::from(dropoff.footprint.min.x - center.x);
        let dy = i64::from(dropoff.footprint.min.y - center.y);
        assert!(
            dx * dx + dy * dy > i64::from(crate::VASSAL_RULES.radius_tiles).pow(2),
            "restored local Mill must remain outside the managed remote domain"
        );
    }
    // The proposed deposit history is60seconds. Keep a managed full-load
    // group unproductive for100seconds so every old real deposit expires.
    let quiet_ticks = 100 * w.config().tick_hz;
    let mut deposits = 0;
    for _ in 0..quiet_ticks {
        w.advance();
        let next = resource_stock(w, ResourceKind::Food);
        deposits += next.saturating_sub(food);
        assert_eq!(next, food);
        assert_eq!(
            assigned(w, remote, workers, ResourceKind::Food),
            workers.len()
        );
        assert!(
            workers
                .iter()
                .all(|id| w.unit(*id).unwrap().carried.is_some_and(|load| {
                    load.kind == ResourceKind::Food
                        && load.amount >= VILLAGE_RULES.villager_capacity
                }))
        );
    }
    assert_eq!(deposits, 0);
    (deposits, earned)
}

fn remote_income_world(kind: ResourceKind, stalled: bool) -> RemoteIncome {
    assert!(!stalled || kind == ResourceKind::Food);
    let (mut w, local, local_workers, node) = wood_realm(20);
    let local_castle = w.vassal_castle(local).unwrap();
    w.issue_ungarrison(PlayerId(0), local_castle, Some(local))
        .unwrap();
    let source = remote_wood(&w, node.tile);
    let castle = place_near(
        &mut w,
        BuildingKind::Castle,
        TileCoord::new(source.tile.x + 14, source.tile.y + 14),
    );
    let footprint = w.building(castle).unwrap().footprint;
    let exit = (footprint.min.y - 1..=footprint.max.y)
        .flat_map(|y| (footprint.min.x - 1..=footprint.max.x).map(move |x| TileCoord::new(x, y)))
        .find(|tile| w.tile_free(*tile))
        .unwrap();
    let remote = w
        .spawn_kind(PlayerId(0), UnitKind::Vassal, position(exit))
        .unwrap();
    w.bind_vassal(remote, castle, footprint);
    // Complete drop-offs before residents exist, so their chosen initial work
    // tiles cannot end up inside a newly inserted Camp or Mill footprint.
    let dropoff = if kind == ResourceKind::Food {
        place_near(&mut w, BuildingKind::Mill, footprint.min)
    } else {
        place_near(&mut w, BuildingKind::LumberCamp, source.tile)
    };
    let remote_workers = workers_near(&mut w, source.tile, 20);
    assert!(
        remote_workers.iter().all(|id| {
            !w.building(dropoff)
                .unwrap()
                .footprint
                .contains(w.unit(*id).unwrap().position.tile_floor())
        }),
        "remote workers must start outside the completed drop-off"
    );
    assert!(w.vassal_status(remote).is_none());
    let (deposits, warmup_deposits) = if stalled {
        stalled_food(&mut w, remote, castle, &remote_workers)
    } else {
        let deposits = if kind == ResourceKind::Food {
            reserves(&mut w, 0, 30_000, 30_000, 30_000);
            w.issue_garrison(PlayerId(0), remote, castle).unwrap();
            make_farmers(&mut w, &remote_workers, footprint.min)
        } else {
            reserves(&mut w, 30_000, 0, 30_000, 30_000);
            let mut setup_ticks = 0;
            for worker in &remote_workers {
                let mut admitted = false;
                let mut attempted = 0;
                for _ in 0..8 {
                    // Replenish ordinary shared path work between attempts.
                    w.advance();
                    setup_ticks += 1;
                    let from = w.unit(*worker).unwrap().position.tile_floor();
                    let mut tried = BTreeSet::new();
                    for _ in 0..64 {
                        let Some(node) = w.nearest_resource_matching(from, kind, 10, None, |n| {
                            !tried.contains(&n.id)
                        }) else {
                            break;
                        };
                        tried.insert(node.id);
                        attempted += 1;
                        if w.issue_gather(PlayerId(0), *worker, node.id).is_ok() {
                            admitted = true;
                            break;
                        }
                    }
                    if admitted {
                        break;
                    }
                }
                assert!(
                    admitted,
                    "remote worker {worker:?} needs a reachable ordinary {kind:?} job; tried {attempted}"
                );
            }
            assert!(
                w.vassal_status(remote).is_none(),
                "ordinary Wood setup precedes management"
            );
            w.issue_garrison(PlayerId(0), remote, castle).unwrap();
            advance(&mut w, 3_000 - setup_ticks);
            observed_deposits(&mut w, 1_000, kind)
        };
        assert!(
            deposits >= 200,
            "remote {kind:?} group must actually deposit during the last50seconds: {deposits}"
        );
        (deposits, deposits)
    };
    assert!(assigned(&w, remote, &remote_workers, kind) >= 18);
    w.issue_garrison(PlayerId(0), local, local_castle).unwrap();
    advance(&mut w, 2);
    assert!(w.vassal_status(local).is_some());
    // Identical current stocks in every comparison, balanced to the existing
    // 45:30 Food:Wood mix. This does not fabricate production/deposit events.
    reserves(&mut w, 1_500, 1_000, 30_000, 30_000);
    RemoteIncome {
        w,
        local,
        local_workers,
        remote,
        remote_workers,
        deposits,
        warmup_deposits,
    }
}

fn first_joint_pass(a: &mut RemoteIncome, b: &mut RemoteIncome) {
    for _ in 0..20 {
        a.w.advance();
        b.w.advance();
        if a.w.vassal_work().domains == 2 && b.w.vassal_work().domains == 2 {
            return;
        }
    }
    panic!("both local domains must receive a manager pass");
}

fn food_jobs(group: &RemoteIncome) -> usize {
    group
        .local_workers
        .iter()
        .filter(|id| {
            group.w.gathering_kind(**id) == Some(ResourceKind::Food)
                || group
                    .w
                    .economy
                    .builders
                    .get(*id)
                    .and_then(|job| group.w.building(job.building))
                    .is_some_and(|b| b.kind == BuildingKind::Farm)
        })
        .count()
}

#[test]
fn income_from_another_vassal_changes_local_jobs_at_equal_kingdom_stock() {
    let mut food = remote_income_world(ResourceKind::Food, false);
    let mut wood = remote_income_world(ResourceKind::Wood, false);
    assert!(food.deposits >= 200 && wood.deposits >= 200);
    assert_eq!(
        food.w.player_economy(PlayerId(0)).unwrap().stockpile,
        wood.w.player_economy(PlayerId(0)).unwrap().stockpile
    );
    // Inspect first decisions before the remote domains can also rebalance
    // their established jobs in response to the newly equal current stocks.
    first_joint_pass(&mut food, &mut wood);
    let food_wood = assigned(&food.w, food.local, &food.local_workers, ResourceKind::Wood);
    let wood_wood = assigned(&wood.w, wood.local, &wood.local_workers, ResourceKind::Wood);
    assert!(
        food_wood > wood_wood,
        "actual remote Food deposits must favor local Wood"
    );
    assert!(
        food_jobs(&wood) > food_jobs(&food),
        "actual remote Wood deposits must favor local Food"
    );
}

#[test]
fn stalled_remote_food_jobs_do_not_count_as_income_without_deposits() {
    let mut productive = remote_income_world(ResourceKind::Food, false);
    let mut stalled = remote_income_world(ResourceKind::Food, true);
    assert!(productive.deposits >= 200);
    assert!(
        stalled.warmup_deposits >= 200,
        "test real history expiry, not a never-productive group"
    );
    assert_eq!(stalled.deposits, 0);
    assert_eq!(
        assigned(
            &productive.w,
            productive.remote,
            &productive.remote_workers,
            ResourceKind::Food
        ),
        20
    );
    assert_eq!(
        assigned(
            &stalled.w,
            stalled.remote,
            &stalled.remote_workers,
            ResourceKind::Food
        ),
        20
    );
    assert_eq!(
        productive.w.player_economy(PlayerId(0)).unwrap().stockpile,
        stalled.w.player_economy(PlayerId(0)).unwrap().stockpile
    );
    first_joint_pass(&mut productive, &mut stalled);
    assert_eq!(
        resource_stock(&stalled.w, ResourceKind::Food),
        1_500,
        "the still-stalled group must not deposit before the inspected decision"
    );
    let productive_wood = assigned(
        &productive.w,
        productive.local,
        &productive.local_workers,
        ResourceKind::Wood,
    );
    let stalled_wood = assigned(
        &stalled.w,
        stalled.local,
        &stalled.local_workers,
        ResourceKind::Wood,
    );
    assert!(
        productive_wood > stalled_wood,
        "equal Food job counts must not imply equal income"
    );
    assert!(
        food_jobs(&stalled) > food_jobs(&productive),
        "zero actual Food income leaves local Food demand"
    );
}

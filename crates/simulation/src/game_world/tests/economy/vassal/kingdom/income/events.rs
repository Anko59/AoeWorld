use super::*;

fn empty_tc(w: &mut GameWorld, anchor: TileCoord) -> EntityId {
    for radius in 0_i32..32 {
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx.abs().max(dy.abs()) != radius {
                    continue;
                }
                let at = TileCoord::new(anchor.x + dx, anchor.y + dy);
                let rect = aoe_core::TileRect::from_xywh(at.x, at.y, 4, 4);
                if w.validate_building_placement(BuildingKind::TownCenter, at)
                    .is_ok()
                    && w.query(rect).0.is_empty()
                {
                    return w
                        .insert_building(PlayerId(0), BuildingKind::TownCenter, rect, true)
                        .unwrap();
                }
            }
        }
    }
    panic!("fixture needs an empty legal Town Center site");
}

#[test]
fn an_equal_stock_training_refund_cycle_does_not_create_income_or_change_priority() {
    let mut plain = remote_income_world(ResourceKind::Food, true);
    let mut refunded = remote_income_world(ResourceKind::Food, true);
    let a_castle = plain.w.vassal_castle(plain.local).unwrap();
    let b_castle = refunded.w.vassal_castle(refunded.local).unwrap();
    let a_at = plain.w.building(a_castle).unwrap().footprint.min;
    let b_at = refunded.w.building(b_castle).unwrap().footprint.min;
    empty_tc(&mut plain.w, TileCoord::new(a_at.x - 12, a_at.y - 12));
    let center = empty_tc(&mut refunded.w, TileCoord::new(b_at.x - 12, b_at.y - 12));
    let before = refunded.w.player_economy(PlayerId(0)).unwrap().stockpile;
    for _ in 0..32 {
        refunded
            .w
            .issue_train(PlayerId(0), center, UnitKind::Villager)
            .unwrap();
        refunded
            .w
            .issue_cancel_training(PlayerId(0), center)
            .unwrap();
    }
    assert_eq!(
        refunded.w.player_economy(PlayerId(0)).unwrap().stockpile,
        before
    );
    assert_eq!(
        plain.w.player_economy(PlayerId(0)).unwrap().stockpile,
        before
    );
    assert!(
        refunded
            .w
            .building(center)
            .unwrap()
            .training_queue
            .is_empty()
    );
    first_joint_pass(&mut plain, &mut refunded);
    assert!(
        food_jobs(&plain) > 0,
        "zero actual Food income leaves local Food demand"
    );
    assert_eq!(food_jobs(&plain), food_jobs(&refunded));
    assert_eq!(
        assigned(
            &plain.w,
            plain.local,
            &plain.local_workers,
            ResourceKind::Wood
        ),
        assigned(
            &refunded.w,
            refunded.local,
            &refunded.local_workers,
            ResourceKind::Wood
        )
    );
}

fn suspend_managers(group: &mut RemoteIncome) -> EntityId {
    let local = group.w.vassal_castle(group.local).unwrap();
    let remote = group.w.vassal_castle(group.remote).unwrap();
    group
        .w
        .issue_ungarrison(PlayerId(0), local, Some(group.local))
        .unwrap();
    group
        .w
        .issue_ungarrison(PlayerId(0), remote, Some(group.remote))
        .unwrap();
    for worker in &group.remote_workers {
        let at = group.w.unit(*worker).unwrap().position;
        let destination = distant_manual_destination(&group.w, at);
        group.w.issue_move(*worker, destination).unwrap();
    }
    local
}

#[test]
fn real_carried_garrison_food_counts_as_income_once() {
    let mut deposited = remote_income_world(ResourceKind::Food, true);
    let mut control = remote_income_world(ResourceKind::Food, true);
    let remote_castle = deposited.w.vassal_castle(deposited.remote).unwrap();
    let remote_at = deposited.w.building(remote_castle).unwrap().footprint.min;
    let a_local = suspend_managers(&mut deposited);
    let b_local = suspend_managers(&mut control);
    let center = empty_tc(&mut deposited.w, remote_at);
    empty_tc(&mut control.w, remote_at);
    let party: Vec<_> = deposited.remote_workers[..12].to_vec();
    let carried: u32 = party
        .iter()
        .map(|id| {
            let load = deposited.w.unit(*id).unwrap().carried.unwrap();
            assert_eq!(load.kind, ResourceKind::Food);
            u32::from(load.amount)
        })
        .sum();
    let before = resource_stock(&deposited.w, ResourceKind::Food);
    for worker in &party {
        deposited
            .w
            .issue_garrison(PlayerId(0), *worker, center)
            .unwrap();
    }
    let mut entered = false;
    for _ in 0..4_000 {
        deposited.w.advance();
        control.w.advance();
        if party.iter().all(|id| {
            deposited
                .w
                .building(center)
                .unwrap()
                .garrison
                .iter()
                .any(|g| g.unit.id == *id)
        }) {
            entered = true;
            break;
        }
    }
    assert!(
        entered,
        "ordinary garrison paths must receive the carried Food"
    );
    assert_eq!(
        resource_stock(&deposited.w, ResourceKind::Food),
        before + carried
    );
    assert_eq!(resource_stock(&control.w, ResourceKind::Food), before);
    for id in &party {
        let occupant = deposited
            .w
            .building(center)
            .unwrap()
            .garrison
            .iter()
            .find(|g| g.unit.id == *id)
            .unwrap();
        assert!(occupant.unit.carried.is_none());
    }
    // Equal present stocks isolate the real garrison deposit history. All
    // remote Food orders were manually released in both worlds beforehand.
    for (group, local) in [(&mut deposited, a_local), (&mut control, b_local)] {
        reserves(&mut group.w, 1_500, 1_000, 30_000, 30_000);
        group
            .w
            .issue_garrison(PlayerId(0), group.local, local)
            .unwrap();
        group.w.advance_garrisons(&mut Vec::new());
        assert!(group.w.vassal_status(group.local).is_some());
    }
    let mut ran = false;
    for _ in 0..20 {
        deposited.w.advance();
        control.w.advance();
        if deposited.w.vassal_work().domains == 1 && control.w.vassal_work().domains == 1 {
            ran = true;
            break;
        }
    }
    assert!(ran);
    assert!(
        assigned(
            &deposited.w,
            deposited.local,
            &deposited.local_workers,
            ResourceKind::Wood
        ) > assigned(
            &control.w,
            control.local,
            &control.local_workers,
            ResourceKind::Wood
        )
    );
    assert!(food_jobs(&control) > food_jobs(&deposited));
}

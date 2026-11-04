use super::*;

#[test]
fn foundations_get_a_few_nearby_helpers_instead_of_the_idle_group() {
    for kind in [
        BuildingKind::House,
        BuildingKind::Mill,
        BuildingKind::Farm,
        BuildingKind::LumberCamp,
        BuildingKind::MiningCamp,
    ] {
        let (mut w, _, _) = manager::active();
        w.set_fog_disabled(true);
        let old: Vec<_> = w.units().map(|u| u.id).collect();
        for id in old {
            w.kill_unit(w.lookup[&id]);
            w.remove_unit(id);
        }
        for x in [10, 13, 16, 19, 22, 25] {
            w.insert_building(
                PlayerId(0),
                BuildingKind::House,
                aoe_core::TileRect::from_xywh(x, 150, 2, 2),
                true,
            )
            .unwrap();
        }
        if kind == BuildingKind::Mill {
            w.insert_building(
                PlayerId(0),
                BuildingKind::Mill,
                aoe_core::TileRect::from_xywh(120, 100, 2, 2),
                true,
            )
            .unwrap();
        }
        let foundation = w
            .insert_building(
                PlayerId(0),
                kind,
                aoe_core::TileRect::from_xywh(100, 100, kind.rules().side, kind.rules().depth),
                false,
            )
            .unwrap();
        // Lower IDs are deliberately farther away. A nearby helper must win
        // even when those villagers are visited first by the domain census.
        let far: Vec<_> = (0..20)
            .map(|i| {
                w.spawn_villager(
                    PlayerId(0),
                    WorldPosition::from_tile_center(TileCoord::new(120 + i % 5, 110 + i / 5))
                        .unwrap(),
                )
                .unwrap()
            })
            .collect();
        let near: Vec<_> = (0..5)
            .map(|i| {
                w.spawn_villager(
                    PlayerId(0),
                    WorldPosition::from_tile_center(TileCoord::new(97 - i, 100)).unwrap(),
                )
                .unwrap()
            })
            .collect();
        manager::pass(&mut w);
        let helpers: Vec<_> = w
            .economy
            .builders
            .iter()
            .filter(|(_, task)| task.building == foundation)
            .map(|(id, _)| *id)
            .collect();
        let limit = match kind {
            BuildingKind::Farm => 1,
            BuildingKind::LumberCamp | BuildingKind::MiningCamp => 4,
            _ => 3,
        };
        assert!(
            !helpers.is_empty(),
            "an unattended {kind:?} needs a builder"
        );
        assert!(
            helpers.len() <= limit,
            "{kind:?} drew {} helpers",
            helpers.len()
        );
        assert!(
            helpers.contains(&near[0]),
            "closest idle helper must win: {helpers:?}"
        );
        assert!(
            helpers.iter().all(|id| !far.contains(id)),
            "distant idle workers must keep looking for productive work"
        );
        if kind == BuildingKind::Mill {
            manager::pass(&mut w);
            assert!(
                far.iter().any(|id| w.gathering_kind(*id).is_some()
                    || w.economy.builders.get(id).is_some_and(|task| w
                        .building(task.building)
                        .unwrap()
                        .kind
                        == BuildingKind::Farm)),
                "a pending Mill must not idle workers beside a completed food drop-off: {:?}, {:?}, {:?}",
                w.steward_work(),
                w.player_economy(PlayerId(0)),
                w.buildings()
                    .map(|b| (b.id, b.kind, b.footprint, b.is_complete()))
                    .collect::<Vec<_>>()
            );
        }
        advance(&mut w, 1_200);
        assert!(
            w.building(foundation).unwrap().is_complete(),
            "bounded helpers still finish {kind:?}"
        );
    }
}

#[test]
fn a_partial_census_counts_builders_from_the_previous_slice() {
    let (mut w, _, steward) = manager::active();
    w.set_fog_disabled(true);
    let old: Vec<_> = w.units().map(|u| u.id).collect();
    for id in old {
        w.kill_unit(w.lookup[&id]);
        w.remove_unit(id);
    }
    let foundation = w
        .insert_building(
            PlayerId(0),
            BuildingKind::House,
            aoe_core::TileRect::from_xywh(100, 100, 2, 2),
            false,
        )
        .unwrap();
    let builders: Vec<_> = (0..3)
        .map(|i| {
            w.spawn_villager(
                PlayerId(0),
                WorldPosition::from_tile_center(TileCoord::new(90 + i, 100)).unwrap(),
            )
            .unwrap()
        })
        .collect();
    w.issue_construct(PlayerId(0), &builders, foundation)
        .unwrap();
    manager::pass(&mut w);
    assert!(
        builders
            .iter()
            .all(|id| w.steward_controller(*id) == Some(steward))
    );
    // The first slice sees the builders, then spends its census on soldiers.
    // The next slice reaches idle workers in the adjacent spatial bucket.
    for _ in 0..1_200 {
        w.spawn_kind(
            PlayerId(0),
            UnitKind::Scout,
            WorldPosition::from_tile_center(TileCoord::new(90, 100)).unwrap(),
        )
        .unwrap();
    }
    for i in 0..8 {
        w.spawn_villager(
            PlayerId(0),
            WorldPosition::from_tile_center(TileCoord::new(104 + i, 100)).unwrap(),
        )
        .unwrap();
    }
    for _ in 0..3 {
        manager::pass(&mut w);
        let count = w
            .economy
            .builders
            .values()
            .filter(|task| task.building == foundation)
            .count();
        assert_eq!(
            count, 3,
            "an incomplete census must not recruit a fresh team"
        );
    }
    assert!(
        !w.building(foundation).unwrap().is_complete(),
        "exercise a live foundation"
    );
}

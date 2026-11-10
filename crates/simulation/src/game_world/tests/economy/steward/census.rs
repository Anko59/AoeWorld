use super::*;

#[test]
fn a_small_economy_replaces_depleted_wood_and_keeps_training_despite_distant_buildings() {
    let (mut w, worker) =
        GameWorld::behaviour_demo(aoe_core::Seed(7), crate::BehaviourScene::Camps).unwrap();
    let steward = w.units().find(|u| u.kind == UnitKind::Steward).unwrap().id;
    let tc = w
        .buildings()
        .find(|b| b.kind == BuildingKind::TownCenter)
        .unwrap()
        .id;
    let camps: Vec<_> = w
        .buildings()
        .filter(|b| b.kind == BuildingKind::LumberCamp)
        .map(|b| b.id)
        .collect();
    let old = camps[0];
    w.destroy_building(camps[1]);
    for _ in 0..4 {
        w.spawn_villager(PlayerId(0), w.unit(worker).unwrap().position)
            .unwrap();
    }
    assert_eq!(
        w.units().filter(|u| u.kind == UnitKind::Villager).count(),
        5
    );
    assert!(
        w.units()
            .all(|u| matches!(u.kind, UnitKind::Villager | UnitKind::Steward))
    );
    for x in [110, 114] {
        w.insert_building(
            PlayerId(0),
            BuildingKind::House,
            aoe_core::TileRect::from_xywh(x, 110, 2, 2),
            true,
        )
        .unwrap();
    }
    let home = w.economy.steward_homes[&steward].clone();
    let sites: Vec<_> = (4..252)
        .flat_map(|y| (212..252).map(move |x| TileCoord::new(x, y)))
        .filter(|tile| {
            (i64::from(tile.x * 1024 + 512 - home.center.x).pow(2)
                + i64::from(tile.y * 1024 + 512 - home.center.y).pow(2))
                > (100 * 1024_i64).pow(2)
                && w.validate_building_placement(BuildingKind::PalisadeWall, *tile)
                    .is_ok()
        })
        .take(crate::STEWARD_RULES.building_visits_per_domain + 1)
        .collect();
    assert_eq!(
        sites.len(),
        crate::STEWARD_RULES.building_visits_per_domain + 1
    );
    for tile in sites {
        w.insert_building(
            PlayerId(0),
            BuildingKind::PalisadeWall,
            aoe_core::TileRect::from_xywh(tile.x, tile.y, 1, 1),
            true,
        )
        .unwrap();
    }
    w.issue_steward_settings(
        PlayerId(0),
        steward,
        crate::StewardSettings {
            weights: [0, 100, 0, 0],
            cap: 12,
            training: true,
        },
    )
    .unwrap();
    let old_tree = ResourceNode::starting(TileCoord::new(126, 131), ResourceKind::Wood).unwrap();
    if let crate::Terrain::Map { generator, overlay } = &mut w.terrain {
        overlay
            .insert_starting_resource(generator, old_tree)
            .unwrap();
    }
    // A replacement woodline must outlast its camp's ordinary construction.
    // The video encounter has only one tree, enough for a deletion clip.
    if let crate::Terrain::Map { generator, overlay } = &mut w.terrain {
        for (x, y) in [
            (135, 129),
            (135, 132),
            (138, 129),
            (138, 132),
            (141, 129),
            (141, 132),
            (144, 132),
        ] {
            overlay
                .insert_starting_resource(
                    generator,
                    ResourceNode::starting(TileCoord::new(x, y), ResourceKind::Wood).unwrap(),
                )
                .unwrap();
        }
    }
    w.resource_chunks.get_mut().unwrap().clear();
    w.resource_collision_changed();
    advance(&mut w, 2);
    w.issue_gather(PlayerId(0), worker, old_tree.id).unwrap();
    advance(&mut w, 100);
    w.deplete_resource(old_tree.id, u16::MAX).unwrap();
    let mut replacement_complete = false;
    let mut villagers_when_replaced = 0;
    for _ in 0..3_000 {
        w.advance();
        if !replacement_complete
            && w.buildings()
                .any(|b| b.kind == BuildingKind::LumberCamp && b.id != old && b.is_complete())
        {
            replacement_complete = true;
            villagers_when_replaced = w.units().filter(|u| u.kind == UnitKind::Villager).count();
        }
        if replacement_complete
            && w.units().filter(|u| u.kind == UnitKind::Villager).count() > villagers_when_replaced
            && !w.building(tc).unwrap().training_queue.is_empty()
        {
            break;
        }
    }
    assert!(
        replacement_complete,
        "small economy must complete a replacement camp: {:?}; camps {:?}; workers {:?}",
        w.steward_status(steward),
        w.buildings()
            .filter(|b| b.kind == BuildingKind::LumberCamp)
            .map(|b| (b.id, b.footprint, b.work_done, b.work_total))
            .collect::<Vec<_>>(),
        w.units()
            .map(|u| (
                u.id,
                u.position.tile_floor(),
                u.task,
                w.gather_tile(u.id),
                w.economy.builders.get(&u.id)
            ))
            .collect::<Vec<_>>()
    );
    assert!(
        w.units().filter(|u| u.kind == UnitKind::Villager).count() > villagers_when_replaced,
        "villager production must continue after replacement"
    );
    assert!(
        !w.building(tc).unwrap().training_queue.is_empty(),
        "keep the next villager queued"
    );
}

#[test]
fn a_new_shelter_behind_the_scan_cursor_still_counts_its_residents() {
    let (mut w, tc, steward) = manager::active();
    let worker = w
        .spawn_villager(
            PlayerId(0),
            WorldPosition::from_tile_center(TileCoord::new(109, 100)).unwrap(),
        )
        .unwrap();
    let villagers = w.units().filter(|u| u.kind == UnitKind::Villager).count();
    w.issue_steward_settings(
        PlayerId(0),
        steward,
        crate::StewardSettings {
            cap: villagers as u8,
            training: true,
            ..Default::default()
        },
    )
    .unwrap();
    for i in 0..crate::STEWARD_RULES.building_visits_per_domain + 1 {
        w.insert_building(
            PlayerId(0),
            BuildingKind::PalisadeWall,
            aoe_core::TileRect::from_xywh(212 + i as i32 % 40, 4 + i as i32 / 40, 1, 1),
            true,
        )
        .unwrap();
    }
    manager::pass(&mut w);
    assert_eq!(
        w.steward_status(steward).unwrap().wait,
        crate::StewardWait::Census
    );
    let shelter = w
        .insert_building(
            PlayerId(0),
            BuildingKind::TownCenter,
            aoe_core::TileRect::from_xywh(110, 100, 4, 4),
            true,
        )
        .unwrap();
    w.issue_garrison(PlayerId(0), worker, shelter).unwrap();
    advance(&mut w, 2);
    assert!(
        w.building(shelter)
            .unwrap()
            .garrison
            .iter()
            .any(|g| g.unit.id == worker)
    );
    manager::pass(&mut w);
    let status = w.steward_status(steward).unwrap();
    assert_eq!(usize::from(status.residents), villagers);
    assert_eq!(status.wait, crate::StewardWait::Full);
    assert!(w.building(tc).unwrap().training_queue.is_empty());
    assert!(w.building(shelter).unwrap().training_queue.is_empty());
}

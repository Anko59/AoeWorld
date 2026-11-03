use super::*;

#[test]
fn an_unclaimed_worker_never_falls_back_to_a_sibling_vassals_tree() {
    let (package, pages, water) = fixture::flat_fixture(false);
    let generator = package
        .generator_with_environment(pages.clone(), water.clone(), Vec::new(), Vec::new())
        .unwrap();
    let offset = (0..=48)
        .step_by(8)
        .find(|dx| {
            [TileCoord::new(125 + dx, 125), TileCoord::new(129 + dx, 125)]
                .iter()
                .all(|t| generator.object_at(*t).is_none())
        })
        .expect("two bare resource sites");
    let own =
        ResourceNode::starting(TileCoord::new(125 + offset, 125), ResourceKind::Wood).unwrap();
    let sibling =
        ResourceNode::starting(TileCoord::new(129 + offset, 125), ResourceKind::Wood).unwrap();
    let mut natural = Vec::new();
    for y in 0..8 {
        for x in 0..8 {
            natural.extend(generator.chunk(x, y).unwrap().resources);
        }
    }
    let mut w =
        GameWorld::from_prepared_map(package, pages, water, Vec::new(), Vec::new()).unwrap();
    w.set_fog_disabled(true);
    for node in natural {
        w.deplete_resource(node.id, node.initial_amount).unwrap();
    }
    if let Terrain::Map { generator, overlay } = &mut w.terrain {
        overlay.insert_starting_resource(generator, own).unwrap();
        overlay
            .insert_starting_resource(generator, sibling)
            .unwrap();
    }
    w.resource_chunks.get_mut().unwrap().clear();
    w.resource_collision_changed();
    let mut kings = Vec::new();
    for x in [100 + offset, 150 + offset] {
        let castle = w
            .insert_building(
                PlayerId(0),
                BuildingKind::Castle,
                aoe_core::TileRect::from_xywh(x, 90, 4, 4),
                true,
            )
            .unwrap();
        let king = w
            .spawn_kind(
                PlayerId(0),
                UnitKind::Vassal,
                position(TileCoord::new(x - 1, 90)),
            )
            .unwrap();
        w.issue_garrison(PlayerId(0), king, castle).unwrap();
        kings.push(king);
    }
    advance(&mut w, 2);
    assert!(w.economy.vassal_homes[&kings[0]].owns(position(own.tile)));
    assert!(w.economy.vassal_homes[&kings[1]].owns(position(sibling.tile)));
    for dx in -1_i32..=1 {
        for dy in -1_i32..=1 {
            if dx == 0 && dy == 0 {
                continue;
            }
            w.insert_building(
                PlayerId(0),
                BuildingKind::StoneWall,
                aoe_core::TileRect::from_xywh(own.tile.x + dx, own.tile.y + dy, 1, 1),
                true,
            )
            .unwrap();
        }
    }
    w.insert_building(
        PlayerId(0),
        BuildingKind::LumberCamp,
        aoe_core::TileRect::from_xywh(119 + offset, 124, 2, 2),
        true,
    )
    .unwrap();
    let worker = w
        .spawn_villager(PlayerId(0), position(TileCoord::new(122 + offset, 125)))
        .unwrap();
    reserves(&mut w, 30_000, 0, 30_000, 30_000);
    assert!(w.resource_has_work_spot(worker, sibling.tile));
    assert_eq!(w.vassal_controller(worker), None);
    // Manual orders may use that reachable alternative. This proves the
    // fixture exercises replacement, rather than mere refusal of all work.
    w.issue_gather(PlayerId(0), worker, own.id).unwrap();
    assert_eq!(w.gather_target(worker), Some(sibling.id));
    w.issue_stop(PlayerId(0), worker).unwrap();
    w.refill_route_budgets();
    // Isolate the issuing pass: later crossing the boundary must not hide
    // an illegal assignment by making the controller disappear.
    w.tick.0 = crate::VASSAL_RULES.period_ticks;
    w.economy.vassals.automatic = true;
    w.advance_vassals(&mut Vec::new());
    w.economy.vassals.automatic = false;
    assert_ne!(
        w.gather_target(worker),
        Some(sibling.id),
        "automatic replacement must observe territory before worker adoption"
    );
}

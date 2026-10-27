use super::*;

fn worksite(
    kind: aoe_map::ResourceKind,
    trees: i32,
    count: i32,
) -> (GameWorld, Vec<EntityId>, Vec<aoe_map::ResourceNode>) {
    let (package, pages, water) = economy::fixture::flat_fixture(false);
    let mut w =
        GameWorld::from_prepared_map(package, pages, water, Vec::new(), Vec::new()).unwrap();
    w.set_fog_disabled(true);
    let Terrain::Map { generator, overlay } = &mut w.terrain else {
        unreachable!()
    };
    let origin = (40..210)
        .flat_map(|y| (40..210).map(move |x| TileCoord::new(x, y)))
        .find(|at| {
            (-10..=8).all(|dx| {
                (-5..=5).all(|dy| {
                    let t = TileCoord::new(at.x + dx, at.y + dy);
                    generator.object_at(t).is_none()
                        && generator.tile_at(t).is_some_and(|t| t.passable)
                })
            })
        })
        .unwrap();
    let nodes = (0..trees)
        .map(|n| {
            let node =
                aoe_map::ResourceNode::starting(TileCoord::new(origin.x + 2 * n, origin.y), kind)
                    .unwrap();
            overlay.insert_starting_resource(generator, node).unwrap();
            node
        })
        .collect::<Vec<_>>();
    let ids = (0..count)
        .map(|n| {
            w.spawn_villager(PlayerId(0), at(origin.x - 6 - n / 5, origin.y - 2 + n % 5))
                .unwrap()
        })
        .collect::<Vec<_>>();
    for (n, &id) in ids.iter().enumerate() {
        w.issue_gather(PlayerId(0), id, nodes[n % nodes.len()].id)
            .unwrap();
    }
    (w, ids, nodes)
}

fn assert_work_progress(mut w: GameWorld, ids: Vec<EntityId>, nodes: Vec<aoe_map::ResourceNode>) {
    let mut worked = BTreeSet::new();
    let mut previous = ids
        .iter()
        .map(|&id| (w.unit(id).unwrap().position, 0))
        .collect::<Vec<_>>();
    for tick in 0..1200 {
        w.advance();
        for (n, &id) in ids.iter().enumerate() {
            let u = w.unit(id).unwrap();
            if u.task == UnitTask::Gathering {
                let node = nodes
                    .iter()
                    .find(|node| Some(node.id) == w.gather_target(id))
                    .expect("must keep the assigned worksite");
                let rect = TileRect::from_xywh(node.tile.x, node.tile.y, 1, 1);
                let x = i64::from(u.position.x)
                    - i64::from(u.position.x.clamp(rect.min.x * 1024, rect.max.x * 1024));
                let y = i64::from(u.position.y)
                    - i64::from(u.position.y.clamp(rect.min.y * 1024, rect.max.y * 1024));
                assert!(
                    x * x + y * y <= 310 * 310,
                    "remote work at tick {tick}: {u:?}, node={node:?}"
                );
                if u.carried.is_some() {
                    worked.insert(id);
                }
            }
            if u.position != previous[n].0 || worked.contains(&id) {
                previous[n].1 = 0;
            } else {
                previous[n].1 += 1;
            }
            assert!(
                previous[n].1 <= 200,
                "worker {id:?} stalled at tick {tick}: {u:?}"
            );
            previous[n].0 = u.position;
        }
        for (i, &a) in ids.iter().enumerate() {
            for &b in &ids[i + 1..] {
                assert!(
                    distance(w.unit(a).unwrap().position, w.unit(b).unwrap().position) >= 410 * 410,
                    "workers overlap at tick {tick}: {a:?}/{b:?}"
                );
            }
        }
        if worked.len() == ids.len() {
            break;
        }
    }
    assert_eq!(
        worked.len(),
        ids.len(),
        "every worker must actually collect"
    );
}

#[test]
fn six_lumberjacks_on_three_trees_all_work_at_the_perimeter() {
    let (w, ids, nodes) = worksite(aoe_map::ResourceKind::Wood, 3, 6);
    assert_work_progress(w, ids, nodes);
}

#[test]
fn ten_villagers_work_around_one_gold_mine() {
    let (w, ids, nodes) = worksite(aoe_map::ResourceKind::Gold, 1, 10);
    assert_work_progress(w, ids, nodes);
}

#[test]
fn a_blocked_approach_away_from_the_worksite_keeps_its_route() {
    let (mut w, ids, nodes) = worksite(aoe_map::ResourceKind::Gold, 1, 1);
    let id = ids[0];
    w.advance();
    let start = w.unit(id).unwrap().position;
    // A foreign crowd seals the worker far from its mine: temporary traffic,
    // not a reason to restart the static route or pick another work spot.
    let ring = [
        (-1, -1),
        (0, -1),
        (1, -1),
        (-1, 0),
        (1, 0),
        (-1, 1),
        (0, 1),
        (1, 1),
    ]
    .map(|(dx, dy)| {
        w.spawn_villager(
            PlayerId(1),
            WorldPosition::new(start.x + dx * 640, start.y + dy * 640),
        )
        .unwrap()
    });
    let orders = w.route_scheduler_stats().metrics.orders;
    let destination = w.movement_order(id).map(|o| o.destination);
    for _ in 0..200 {
        w.advance();
    }
    assert!(destination.is_some());
    assert_eq!(w.movement_order(id).map(|o| o.destination), destination);
    assert!(w.route_scheduler_stats().metrics.orders <= orders + 1);
    for other in ring {
        w.remove_unit(other);
    }
    assert_work_progress(w, ids, nodes);
}

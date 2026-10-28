use super::*;

#[test]
fn fishermen_and_ships_face_fish_from_multiple_approach_directions_every_tick() {
    use crate::Facing;
    let mut world = coast();
    let Terrain::Map { generator, .. } = world.terrain() else {
        unreachable!()
    };
    let nodes = (0..5)
        .flat_map(|cy| (0..8).flat_map(move |cx| generator.chunk(cx, cy).unwrap().resources))
        .collect::<Vec<_>>();
    for (object, kind) in [
        (ObjectKind::ShoreFish, UnitKind::Villager),
        (ObjectKind::ShoreFish, UnitKind::FishingShip),
        (ObjectKind::DeepFish, UnitKind::FishingShip),
    ] {
        let node = nodes
            .iter()
            .filter(|n| n.object == object && n.tile.y > 8)
            .max_by_key(|n| {
                (-1..=1)
                    .flat_map(|dy| (-1..=1).map(move |dx| (dx, dy)))
                    .filter(|&(dx, dy)| {
                        (dx != 0 || dy != 0)
                            && world
                                .unit_tile_passable(
                                    kind,
                                    TileCoord::new(n.tile.x + dx, n.tile.y + dy),
                                    &|| false,
                                )
                                .unwrap()
                    })
                    .count()
            })
            .unwrap();
        let mut directions = 0;
        for (dx, dy, expected) in [
            (-1, -1, Facing::South),
            (-1, 0, Facing::SouthEast),
            (-1, 1, Facing::East),
            (0, 1, Facing::NorthEast),
            (1, 1, Facing::North),
            (1, 0, Facing::NorthWest),
            (1, -1, Facing::West),
            (0, -1, Facing::SouthWest),
        ] {
            let tile = TileCoord::new(node.tile.x + dx, node.tile.y + dy);
            let Ok(id) = world.spawn_kind(
                PlayerId(0),
                kind,
                WorldPosition::from_tile_center(tile).unwrap(),
            ) else {
                continue;
            };
            world.issue_gather(PlayerId(0), id, node.id).unwrap();
            let index = world.lookup[&id];
            for _ in 0..200 {
                if world.unit(id).unwrap().task == UnitTask::Fishing {
                    break;
                }
                world.advance();
            }
            for _ in 0..2 {
                world.units[index].state.facing = if expected == Facing::North {
                    Facing::South
                } else {
                    Facing::North
                };
                let changed = world.advance();
                let worker = world.unit(id).unwrap();
                assert!(!worker.moving);
                assert_eq!(worker.task, UnitTask::Fishing);
                assert_eq!(worker.facing, expected, "{object:?} {kind:?} from {tile:?}");
                assert!(changed.iter().any(|u| u.id == id));
            }
            directions += 1;
            world.remove_unit(id);
        }
        assert!(
            directions >= 3,
            "{object:?} {kind:?}: fixture needs several approaches"
        );
    }
}

//! An owner's unit far from its closed gate is routed through it, not around
//! the wall or refused; an enemy stays outside.
use super::*;

/// A closed 11x9 fort at `o` with a horizontal gate in the south side and,
/// when `vertical`, a vertical gate in the east side. Returns the gate ids.
pub(super) fn fort(world: &mut GameWorld, o: TileCoord, vertical: bool) -> Vec<EntityId> {
    let tile = |x, y| TileCoord::new(o.x + x, o.y + y);
    let wall = |world: &mut GameWorld, at: TileCoord| {
        world
            .insert_building(
                PlayerId(0),
                BuildingKind::StoneWall,
                aoe_core::TileRect::from_xywh(at.x, at.y, 1, 1),
                true,
            )
            .unwrap();
    };
    for x in 0..=10 {
        wall(world, tile(x, 0));
        if !(3..7).contains(&x) {
            wall(world, tile(x, 8));
        }
    }
    for y in 1..8 {
        wall(world, tile(0, y));
        if !vertical || !(2..6).contains(&y) {
            wall(world, tile(10, y));
        }
    }
    let mut gates = vec![
        world
            .insert_building(
                PlayerId(0),
                BuildingKind::StoneGate,
                aoe_core::TileRect::from_xywh(o.x + 3, o.y + 8, 4, 1),
                true,
            )
            .unwrap(),
    ];
    if vertical {
        gates.push(
            world
                .insert_building(
                    PlayerId(0),
                    BuildingKind::StoneGate,
                    aoe_core::TileRect::from_xywh(o.x + 10, o.y + 2, 1, 4),
                    true,
                )
                .unwrap(),
        );
    }
    gates
}

fn walk(world: &mut GameWorld, unit: EntityId, to: TileCoord, ticks: u32) -> TileCoord {
    world
        .issue_move(unit, WorldPosition::from_tile_center(to).unwrap())
        .unwrap();
    for _ in 0..ticks {
        world.advance();
        if world.unit(unit).unwrap().position.tile_floor() == to {
            break;
        }
    }
    world.unit(unit).unwrap().position.tile_floor()
}

fn crosses(mut world: GameWorld, o: TileCoord) {
    let gates = fort(&mut world, o, true);
    advance(&mut world, 2);
    assert!(gates.iter().all(|g| !world.building(*g).unwrap().gate_open));
    let inside = TileCoord::new(o.x + 5, o.y + 4);
    // Far from the south gate: the owner is routed to it and walks in.
    let own = world
        .spawn_villager(
            PlayerId(0),
            WorldPosition::from_tile_center(TileCoord::new(o.x + 5, o.y + 20)).unwrap(),
        )
        .unwrap();
    assert_eq!(walk(&mut world, own, inside, 900), inside, "south gate");
    // From the east, through the vertical gate, and back out.
    let east = TileCoord::new(o.x + 22, o.y + 4);
    assert_eq!(walk(&mut world, own, east, 900), east, "east gate out");
    assert_eq!(walk(&mut world, own, inside, 900), inside, "east gate in");
    let away = TileCoord::new(o.x + 5, o.y + 24);
    assert_eq!(walk(&mut world, own, away, 900), away);
    advance(&mut world, 40);
    assert!(gates.iter().all(|g| !world.building(*g).unwrap().gate_open));
    let enemy = world
        .spawn_militia(
            PlayerId(1),
            WorldPosition::from_tile_center(TileCoord::new(o.x + 22, o.y + 12)).unwrap(),
        )
        .unwrap();
    let _ = world.issue_move(enemy, WorldPosition::from_tile_center(inside).unwrap());
    advance(&mut world, 600);
    let at = world.unit(enemy).unwrap().position.tile_floor();
    let rect = aoe_core::TileRect::from_xywh(o.x, o.y, 11, 9);
    assert!(!rect.contains(at), "the enemy stayed outside: {at:?}");
}

#[test]
fn an_owner_far_from_its_closed_gates_walks_through_them_on_a_uniform_world() {
    let (world, _, _) = village();
    crosses(world, TileCoord::new(100, 100));
}

#[test]
fn an_owner_far_from_its_closed_gates_walks_through_them_on_a_map() {
    let (package, pages, water) = fixture::flat_fixture(false);
    let mut world =
        GameWorld::from_prepared_map(package, pages, water, Vec::new(), Vec::new()).unwrap();
    world.set_fog_disabled(true);
    let o = (0..64)
        .map(|i| TileCoord::new(40 + (i % 8) * 20, 40 + (i / 8) * 20))
        .find(|o| {
            (-2..34).all(|dy| {
                (-2..34).all(|dx| world.tile_passable(TileCoord::new(o.x + dx, o.y + dy)))
            })
        })
        .expect("an open flat area");
    crosses(world, o);
}

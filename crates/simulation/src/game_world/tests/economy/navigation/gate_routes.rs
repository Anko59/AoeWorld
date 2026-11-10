//! An owner's unit far from its closed gate is routed through it, not around
//! the wall or refused; an enemy stays outside.
use super::*;

/// A closed 11x9 fort at `o` with a horizontal gate in the south side and,
/// when `vertical`, a vertical gate in the east side. Returns the gate ids.
pub(super) fn fort(world: &mut GameWorld, o: TileCoord, vertical: bool) -> Vec<EntityId> {
    fort_kind(
        world,
        o,
        vertical,
        BuildingKind::StoneWall,
        BuildingKind::StoneGate,
    )
}

fn fort_kind(
    world: &mut GameWorld,
    o: TileCoord,
    vertical: bool,
    wall_kind: BuildingKind,
    gate_kind: BuildingKind,
) -> Vec<EntityId> {
    let tile = |x, y| TileCoord::new(o.x + x, o.y + y);
    let wall = |world: &mut GameWorld, at: TileCoord| {
        world
            .insert_building(
                PlayerId(0),
                wall_kind,
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
                gate_kind,
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
                    gate_kind,
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
    let (world, o) = map_fort_site();
    crosses(world, o);
}

fn map_fort_site() -> (GameWorld, TileCoord) {
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
    (world, o)
}

#[test]
fn owner_can_order_a_move_onto_a_closed_gate_from_far_away() {
    for kind in [BuildingKind::PalisadeGate, BuildingKind::StoneGate] {
        let (mut world, _, _) = village();
        let gate = world
            .insert_building(
                PlayerId(0),
                kind,
                aoe_core::TileRect::from_xywh(100, 100, 4, 1),
                true,
            )
            .unwrap();
        advance(&mut world, 2);
        let unit = world
            .spawn_villager(
                PlayerId(0),
                WorldPosition::from_tile_center(TileCoord::new(102, 112)).unwrap(),
            )
            .unwrap();
        assert!(!world.building(gate).unwrap().gate_open);
        let target = TileCoord::new(102, 100);
        assert_eq!(walk(&mut world, unit, target, 600), target, "{kind:?}");
        assert!(world.building(gate).unwrap().gate_open);
    }
}

#[test]
fn gate_destinations_reject_foundations_and_closed_enemies_but_allow_open_enemies() {
    for kind in [BuildingKind::PalisadeGate, BuildingKind::StoneGate] {
        let (mut world, _, _) = village();
        let rect = aoe_core::TileRect::from_xywh(100, 100, 4, 1);
        let gate = world
            .insert_building(PlayerId(0), kind, rect, false)
            .unwrap();
        let target = WorldPosition::from_tile_center(TileCoord::new(102, 100)).unwrap();
        let own = world
            .spawn_villager(
                PlayerId(0),
                WorldPosition::from_tile_center(TileCoord::new(102, 110)).unwrap(),
            )
            .unwrap();
        let enemy = world
            .spawn_villager(
                PlayerId(1),
                WorldPosition::from_tile_center(TileCoord::new(102, 90)).unwrap(),
            )
            .unwrap();
        assert!(world.issue_move(own, target).is_err(), "foundation");
        let building = world.building_mut(gate).unwrap();
        building.state = crate::ConstructionState::Complete;
        building.work_done = building.work_total;
        advance(&mut world, 2);
        assert!(!world.building(gate).unwrap().gate_open);
        assert!(world.issue_move(enemy, target).is_err(), "closed enemy");
        assert!(world.issue_queued_move(enemy, target).is_err());
        world.issue_queued_move(own, target).unwrap();
        advance(&mut world, 500);
        assert_eq!(
            world.unit(own).unwrap().position.tile_floor(),
            target.tile_floor()
        );
        let inside = TileCoord::new(101, 98);
        assert_eq!(walk(&mut world, own, inside, 300), inside);
        assert!(world.building(gate).unwrap().gate_open);
        assert_eq!(
            walk(&mut world, enemy, target.tile_floor(), 500),
            target.tile_floor()
        );
        let away = TileCoord::new(102, 115);
        assert_eq!(walk(&mut world, own, away, 600), away);
        assert!(
            world.building(gate).unwrap().gate_open,
            "enemy on threshold holds it open"
        );
        assert_eq!(
            walk(&mut world, enemy, TileCoord::new(102, 90), 600),
            TileCoord::new(102, 90)
        );
        advance(&mut world, 10);
        assert!(
            !world.building(gate).unwrap().gate_open,
            "closes behind the enemy"
        );
    }
}

#[test]
#[cfg(not(target_arch = "wasm32"))]
fn gate_approach_and_closing_replay_identically_on_one_and_four_workers() {
    let setup = || {
        let (mut world, _, _) = village();
        let gates = fort(&mut world, TileCoord::new(100, 100), true);
        let worker = world
            .spawn_villager(
                PlayerId(0),
                WorldPosition::from_tile_center(TileCoord::new(105, 125)).unwrap(),
            )
            .unwrap();
        advance(&mut world, 2);
        world
            .issue_move(
                worker,
                WorldPosition::from_tile_center(TileCoord::new(105, 108)).unwrap(),
            )
            .unwrap();
        (world, worker, gates[0])
    };
    let (mut a, worker, gate) = setup();
    let (mut b, _, _) = setup();
    let one = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .unwrap();
    let four = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .unwrap();
    let mut opened = false;
    for tick in 0..1000 {
        if tick == 500 {
            for world in [&mut a, &mut b] {
                world
                    .issue_move(
                        worker,
                        WorldPosition::from_tile_center(TileCoord::new(105, 125)).unwrap(),
                    )
                    .unwrap();
            }
        }
        one.install(|| a.advance());
        four.install(|| b.advance());
        assert_eq!(a.canonical_hash(), b.canonical_hash(), "tick {tick}");
        opened |= a.building(gate).unwrap().gate_open;
    }
    assert!(opened);
    assert!(!a.building(gate).unwrap().gate_open);
    assert_eq!(
        a.unit(worker).unwrap().position.tile_floor(),
        TileCoord::new(105, 125)
    );
}

#[path = "gate_admission.rs"]
mod admission;

use super::*;
use aoe_core::{Seed, TileCoord, WorldConfig};

#[test]
fn movement_reuses_proposals_and_dense_tables_between_ticks() {
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(0)).unwrap()).unwrap();
    for n in 0..64 {
        let at = WorldPosition::from_tile_center(TileCoord::new(20 + n % 8, 20 + n / 8)).unwrap();
        let id = world.spawn_unit(PlayerId(0), at).unwrap();
        world
            .issue_move(id, WorldPosition::new(at.x + 64 * 1024, at.y))
            .unwrap();
    }
    world.advance();
    let proposals = world.movement_scratch.proposals.as_ptr();
    let busy = world.movement_scratch.tables.busy.as_ptr();
    let spare = world.movement_scratch.movers.as_ptr();
    world.advance();
    world.advance();
    assert_eq!(world.active_mover_count(), 64);
    assert_eq!(world.movement_scratch.proposals.as_ptr(), proposals);
    assert_eq!(world.movement_scratch.tables.busy.as_ptr(), busy);
    assert_eq!(world.movement_scratch.movers.as_ptr(), spare);
}

#[test]
fn an_idle_population_does_not_build_movement_tables() {
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(0)).unwrap()).unwrap();
    for n in 0..1_000 {
        world
            .spawn_unit(
                PlayerId(0),
                WorldPosition::from_tile_center(TileCoord::new(10 + n % 100, 10 + n / 100))
                    .unwrap(),
            )
            .unwrap();
    }
    world.advance();
    assert_eq!(world.movement_scratch.proposals.capacity(), 0);
    assert_eq!(world.movement_scratch.tables.busy.capacity(), 0);
}

#[test]
fn late_game_proposals_use_no_per_unit_neighbour_hash_lookups() {
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(0)).unwrap()).unwrap();
    for n in 0..1_000 {
        world
            .spawn_unit(
                PlayerId(0),
                WorldPosition::from_tile_center(TileCoord::new(100 + n % 100, 100 + n / 100))
                    .unwrap(),
            )
            .unwrap();
    }
    let mut movers = Vec::new();
    for n in 0..64 {
        let at = WorldPosition::from_tile_center(TileCoord::new(20 + n % 8, 20 + n / 8)).unwrap();
        let id = world.spawn_unit(PlayerId(0), at).unwrap();
        world
            .issue_move(id, WorldPosition::new(at.x + 64 * 1024, at.y))
            .unwrap();
        movers.push(id);
    }
    world.followers.reset_neighbour_lookups();
    let mut scratch = Scratch::default();
    world.propose_steps_into(&movers, 7, &mut scratch);
    assert_eq!(scratch.jobs.len(), 64);
    assert_eq!(scratch.followers.cell_count(), 4);
    assert_eq!(world.followers.neighbour_lookups(), 0);
    for turn in 0..64 {
        assert_eq!(
            scratch.take_step(turn).unwrap().state.id,
            movers[(7 + turn) % 64]
        );
    }
}

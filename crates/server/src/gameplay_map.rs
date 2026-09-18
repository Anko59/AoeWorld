use crate::GameplayService;
use aoe_core::{PlayerId, TileCoord, WorldPosition};
use aoe_map::MapPackage;
use aoe_simulation::{GameWorld, GameWorldError};

impl GameplayService {
    pub fn from_map(package: MapPackage) -> Result<Self, GameWorldError> {
        let mut world = GameWorld::from_map(package)?;
        let config = world.config();
        let mut state = config.seed.0.max(1);
        for _ in 0..4_096 {
            state = next_random(&mut state);
            let tile = TileCoord::new(
                (state % config.width_tiles as u64) as i32,
                (state.rotate_left(29) % config.height_tiles as u64) as i32,
            );
            if !world.terrain().passable(tile, config) {
                continue;
            }
            let position = WorldPosition::from_tile_center(tile)
                .map_err(|_| GameWorldError::InvalidPosition)?;
            let primary_unit_id = world.spawn_unit(PlayerId(0), position)?;
            return Ok(Self::from_world(world, primary_unit_id));
        }
        Err(GameWorldError::InvalidPosition)
    }
}

fn next_random(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

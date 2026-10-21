use aoe_core::{EntityId, PlayerId, TileCoord, TileRect};
use aoe_protocol::FogState;
use aoe_simulation::{GameWorld, TileVisibility};

/// Bounded, opt-in film setup. The ordinary starting kit stays unchanged.
pub(super) fn showcase_enemies(world: &mut GameWorld, primary: EntityId) {
    let Some(start) = world.unit(primary).map(|u| u.position.tile_floor()) else {
        return;
    };
    'wolf: for dx in 17..=23 {
        for dy in -3..=3 {
            let tile = TileCoord::new(start.x + dx, start.y + dy);
            if world.visibility(viewer(world, primary), tile) == TileVisibility::Unexplored
                && let Ok(at) = aoe_core::WorldPosition::from_tile_center(tile)
                && world
                    .spawn_animal(aoe_simulation::AnimalKind::Wolf, at)
                    .is_ok()
            {
                break 'wolf;
            }
        }
    }
    // A separate, initially unseen footprint permits real building-memory
    // captures after the scout returns. Ordinary games never enter this setup.
    for dx in 17..=23 {
        for dy in 6..=10 {
            let tile = TileCoord::new(start.x + dx, start.y + dy);
            if world.visibility(viewer(world, primary), tile) == TileVisibility::Unexplored
                && world.place_town_center(PlayerId(1), tile).is_ok()
            {
                eprintln!("fog showcase: enemy Town Center at {},{}", tile.x, tile.y);
                return;
            }
        }
    }
}

pub(super) fn viewer(world: &GameWorld, primary: EntityId) -> PlayerId {
    world.unit(primary).map_or(PlayerId(0), |u| u.player)
}

pub(super) fn state(
    world: &GameWorld,
    player: PlayerId,
    revision: u64,
    region: TileRect,
) -> FogState {
    #[cfg(test)]
    tests::record_state(region);
    let disabled = world.fog_disabled();
    let mut tiles = Vec::new();
    if !disabled {
        tiles.reserve((region.width() * region.height()) as usize);
        for y in region.min.y..region.max.y {
            for x in region.min.x..region.max.x {
                tiles.push(match world.visibility(player, TileCoord::new(x, y)) {
                    TileVisibility::Unexplored => 0,
                    TileVisibility::Explored => 1,
                    TileVisibility::Visible => 2,
                });
            }
        }
    }
    FogState {
        revision,
        tick: world.tick(),
        region,
        disabled,
        tiles,
        overview: if disabled {
            Vec::new()
        } else {
            world.vision_overview(player)
        },
    }
}

#[path = "fog/tests.rs"]
#[cfg(test)]
mod tests;

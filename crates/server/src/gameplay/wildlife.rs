//! Wildlife subscription replacements, independent of unit delta residents.
use aoe_core::TileRect;
use aoe_protocol::{AnimalKind, AnimalState, AnimalTask};
use aoe_simulation::GameWorld;

#[path = "wildlife/tests.rs"]
#[cfg(test)]
mod tests;

pub(super) fn animals(
    world: &GameWorld,
    region: TileRect,
    player: aoe_core::PlayerId,
) -> Vec<AnimalState> {
    world
        .animals()
        .filter(|a| a.owner == Some(player) || region.contains(a.position.tile_floor()))
        .filter(|a| {
            a.owner == Some(player)
                || world.visibility(player, a.position.tile_floor())
                    == aoe_simulation::TileVisibility::Visible
        })
        .map(|a| AnimalState {
            id: a.id,
            kind: match a.kind {
                aoe_simulation::AnimalKind::Sheep => AnimalKind::Sheep,
                aoe_simulation::AnimalKind::Deer => AnimalKind::Deer,
                aoe_simulation::AnimalKind::Boar => AnimalKind::Boar,
                aoe_simulation::AnimalKind::Wolf => AnimalKind::Wolf,
            },
            owner: a.owner,
            hp: a.hp,
            position: a.position,
            task: match a.task {
                aoe_simulation::AnimalTask::Idle => AnimalTask::Idle,
                aoe_simulation::AnimalTask::Fleeing => AnimalTask::Fleeing,
                aoe_simulation::AnimalTask::Attacking => AnimalTask::Attacking,
                aoe_simulation::AnimalTask::Dead => AnimalTask::Dead,
                aoe_simulation::AnimalTask::Carcass => AnimalTask::Carcass,
            },
            food_remaining: a.food_remaining,
            target: a.target,
            facing: a.facing as u8,
            moving: a.moving,
            death_tick: a.death_tick,
        })
        .collect()
}

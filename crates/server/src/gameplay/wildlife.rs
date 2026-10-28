//! Wildlife subscription replacements, independent of unit delta residents.
use aoe_core::TileRect;
use aoe_protocol::{AnimalKind, AnimalState, AnimalTask};
use aoe_simulation::GameWorld;

pub(super) fn projectiles(
    world: &GameWorld,
    region: TileRect,
    player: aoe_core::PlayerId,
) -> Vec<aoe_protocol::ProjectileState> {
    world
        .visible_projectiles(player)
        .filter(|p| region.contains(p.position(world.tick()).tile_floor()))
        .map(|p| {
            // Reveal only the visible remainder: endpoints in black/fog must not
            // disclose an unseen firing position or target. A visible firing
            // position keeps the true launch, so the client can pitch the
            // flight along its arc and time the shooter's release.
            let position = p.position(world.tick());
            let visible = |at: aoe_core::WorldPosition| {
                world.visibility(player, at.tile_floor()) == aoe_simulation::TileVisibility::Visible
            };
            let seen = visible(p.from);
            aoe_protocol::ProjectileState {
                id: p.id,
                attacker: seen.then_some(p.attacker),
                from: if seen { p.from } else { position },
                to: if visible(p.to) { p.to } else { position },
                launched: if seen { p.launched } else { world.tick() },
                impact: p.impact,
                kind: match p.kind {
                    aoe_simulation::ProjectileKind::Stone => aoe_protocol::ProjectileKind::Stone,
                    aoe_simulation::ProjectileKind::Arrow => aoe_protocol::ProjectileKind::Arrow,
                    aoe_simulation::ProjectileKind::Javelin => {
                        aoe_protocol::ProjectileKind::Javelin
                    }
                },
            }
        })
        // Projectiles are shown, never simulated, on the client: a crowded
        // battle shows the first `MAX_PROJECTILES` rather than failing the frame.
        .take(aoe_protocol::MAX_PROJECTILES)
        .collect()
}

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

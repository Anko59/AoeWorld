//! Fog-filtered relic markers for a subscription. Fan-out indexes this
//! sparse, authoritative set once per frame.
use aoe_core::{EntityId, PlayerId, TileCoord, TileRect};
use aoe_protocol::{GroundRelic, MonasteryRelics, RelicVictory, RelicView};
use aoe_simulation::{GameWorld, TileVisibility};

#[derive(Clone, Copy)]
pub(super) enum Marker {
    Ground(GroundRelic),
    Carrier(EntityId, PlayerId, TileCoord),
    Monastery(MonasteryRelics, PlayerId, TileRect),
}

impl Marker {
    pub(super) fn owner(self) -> Option<PlayerId> {
        match self {
            Self::Ground(_) => None,
            Self::Carrier(_, owner, _) | Self::Monastery(_, owner, _) => Some(owner),
        }
    }

    pub(super) fn tile(self) -> TileCoord {
        match self {
            Self::Ground(relic) => relic.position.tile_floor(),
            Self::Carrier(_, _, tile) => tile,
            Self::Monastery(_, _, rect) => rect.min,
        }
    }
}

pub(super) fn markers(world: &GameWorld) -> impl Iterator<Item = Marker> + '_ {
    world
        .ground_relics()
        .map(|(id, position)| Marker::Ground(GroundRelic { id, position }))
        .chain(world.relic_carriers().filter_map(|id| {
            let unit = world.unit(id)?;
            // An embarked Monk has no spatial presence and may still own a relic.
            world
                .unit_visible_to(unit.player, id)
                .then_some(Marker::Carrier(id, unit.player, unit.position.tile_floor()))
        }))
        .chain(world.relic_monasteries().filter_map(|(id, count)| {
            let building = world.building(id)?;
            Some(Marker::Monastery(
                MonasteryRelics {
                    id,
                    count,
                    full: !world.monastery_has_relic_space(id),
                },
                building.player,
                building.footprint,
            ))
        }))
}

pub(super) fn view(
    world: &GameWorld,
    player: PlayerId,
    revision: u64,
    region: TileRect,
    markers: impl Iterator<Item = Marker>,
) -> Option<RelicView> {
    let mut view = RelicView {
        revision,
        ..RelicView::default()
    };
    for marker in markers {
        if marker.owner() != Some(player) && !region.contains(marker.tile()) {
            continue;
        }
        match marker {
            Marker::Ground(relic)
                if world.visibility(player, marker.tile()) == TileVisibility::Visible =>
            {
                view.ground.push(relic)
            }
            Marker::Carrier(id, owner, tile)
                if owner == player || world.visibility(player, tile) == TileVisibility::Visible =>
            {
                view.carriers.push(id)
            }
            Marker::Monastery(relics, owner, rect)
                if owner == player || world.building_visible_to(player, rect) =>
            {
                view.monasteries.push(relics)
            }
            _ => {}
        }
    }
    view.victory = world.relic_victory().map(|status| match status {
        aoe_simulation::RelicVictory::Holding { player, deadline } => RelicVictory {
            leader: player,
            remaining_seconds: deadline
                .0
                .saturating_sub(world.tick().0)
                .div_ceil(u64::from(world.config().tick_hz))
                .max(1) as u16,
        },
        aoe_simulation::RelicVictory::Won(player) => RelicVictory {
            leader: player,
            remaining_seconds: 0,
        },
    });
    (!view.ground.is_empty()
        || !view.carriers.is_empty()
        || !view.monasteries.is_empty()
        || view.victory.is_some())
    .then_some(view)
}

#[cfg(test)]
mod tests;

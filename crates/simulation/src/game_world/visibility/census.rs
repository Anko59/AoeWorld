//! Diagnostics only (#978 slice 1c): how many units each state of the scale
//! plan would hold today. Read-only, never hashed; a report samples it.
use super::views::HumanView;
use crate::GameWorld;
use aoe_core::{PlayerId, SPATIAL_CHUNK_TILES, TileCoord};
use std::collections::BTreeMap;

/// The narrow contact radius of #978 4.3, in tiles.
const NARROW_CONTACT_TILES: i32 = 16;
/// The watched margin around a view at unit zoom, in tiles (#978 4.2).
const WATCH_MARGIN_TILES: i32 = 16;

/// Units by what the scale plan would do with them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct StateCensus {
    pub units: u64,
    /// Units with a movement order.
    pub movers: u64,
    /// Inside a human view at unit zoom, plus the margin.
    pub watched: u64,
    /// Today's full-model rule: a human seat owns or sees the unit's tile.
    pub seen: u64,
    /// Attacking (a target) or on attack-move: the units that make contact.
    pub threats: u64,
    /// Units with a threat of another player (or, being one, any unit of
    /// another player) in their chunk or the eight around it.
    pub contact_simple: u64,
    /// The same within `NARROW_CONTACT_TILES`.
    pub contact_narrow: u64,
}

impl GameWorld {
    /// One pass over every unit; for a report's sample ticks, not the tick.
    pub fn state_census(&self) -> StateCensus {
        let mut census = StateCensus::default();
        let mut by_chunk = BTreeMap::<(i32, i32), Vec<(PlayerId, TileCoord, bool)>>::new();
        for (index, stored) in self.units.iter().enumerate() {
            let unit = &stored.state;
            if unit.hp == 0 {
                continue;
            }
            let tile = unit.position.tile_floor();
            let threat = unit.attack_target.is_some()
                || self.combat.attackers.contains_key(&unit.id)
                || self.offensive_continuation(unit.id).is_some();
            census.units += 1;
            census.movers += u64::from(stored.order.is_some());
            census.threats += u64::from(threat);
            census.seen += u64::from(self.seen_by_human_seat(index));
            census.watched += u64::from(watched(&self.vision.human_views, tile));
            by_chunk
                .entry(chunk(tile))
                .or_default()
                .push((unit.player, tile, threat));
        }
        for (&(cx, cy), units) in &by_chunk {
            for &(player, tile, threat) in units {
                let (mut simple, mut narrow) = (false, false);
                'near: for dy in -1..=1 {
                    for dx in -1..=1 {
                        let Some(near) = by_chunk.get(&(cx + dx, cy + dy)) else {
                            continue;
                        };
                        for &(other, at, other_threat) in near {
                            if other == player || !(threat || other_threat) {
                                continue;
                            }
                            simple = true;
                            if (at.x - tile.x).abs().max((at.y - tile.y).abs())
                                <= NARROW_CONTACT_TILES
                            {
                                narrow = true;
                                break 'near;
                            }
                        }
                    }
                }
                census.contact_simple += u64::from(simple);
                census.contact_narrow += u64::from(narrow);
            }
        }
        census
    }

    fn seen_by_human_seat(&self, index: usize) -> bool {
        let unit = &self.units[index].state;
        let tile = unit.position.tile_floor();
        self.vision.human_seats.iter().any(|&player| {
            unit.player == player || self.visibility(player, tile) == crate::TileVisibility::Visible
        })
    }
}

fn chunk(tile: TileCoord) -> (i32, i32) {
    (
        tile.x.div_euclid(SPATIAL_CHUNK_TILES),
        tile.y.div_euclid(SPATIAL_CHUNK_TILES),
    )
}

fn watched(views: &[HumanView], tile: TileCoord) -> bool {
    views.iter().any(|view| {
        view.unit_zoom
            && tile.x >= view.region.min.x - WATCH_MARGIN_TILES
            && tile.x < view.region.max.x + WATCH_MARGIN_TILES
            && tile.y >= view.region.min.y - WATCH_MARGIN_TILES
            && tile.y < view.region.max.y + WATCH_MARGIN_TILES
    })
}

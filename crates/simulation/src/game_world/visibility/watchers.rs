//! Building memory driven by sight events. A building's remembered snapshot
//! refreshes only for players currently seeing it. A destroyed building leaves
//! memory at once for its owner and its watchers; for everyone else it stays as
//! a ghost until a tile of its site is seen again.
use super::Vision;
use crate::GameWorld;
use aoe_core::{EntityId, PlayerId, TileCoord, TileRect};
use std::collections::BTreeSet;

/// The players currently seeing a live building. The owner is always one of
/// them, so owned destruction is known immediately.
#[derive(Debug)]
pub(super) struct Watched {
    pub(super) owner: PlayerId,
    pub(super) footprint: TileRect,
    pub(super) watchers: BTreeSet<PlayerId>,
}

impl Vision {
    /// Feeds sight gains and losses into the building watcher index. Gains
    /// only watch; the memory refresh happens once per flush. Losses recheck
    /// the whole footprint before a player stops watching.
    pub(super) fn sight_flips(&mut self, player: PlayerId) {
        let flips = std::mem::take(&mut self.flips);
        for (tile, seen) in &flips {
            if *seen
                && let Some(vision) = self.players.get_mut(&player)
                && let Some(ids) = vision.ghost_tiles.get(tile).cloned()
            {
                for id in ids {
                    vision.exorcise(id);
                }
            }
            let Some(&building) = self.building_at_tile.get(tile) else {
                continue;
            };
            if *seen {
                if let Some(watched) = self.building_watchers.get_mut(&building) {
                    watched.watchers.insert(player);
                }
            } else {
                let stop_watching = match self.building_watchers.get(&building) {
                    Some(watched) => {
                        watched.owner != player
                            && watched.watchers.contains(&player)
                            && !self.any_tile_visible(player, watched.footprint)
                    }
                    None => false,
                };
                if stop_watching && let Some(watched) = self.building_watchers.get_mut(&building) {
                    watched.watchers.remove(&player);
                }
            }
        }
        self.flips = flips;
    }

    fn any_tile_visible(&self, player: PlayerId, footprint: TileRect) -> bool {
        let Some(vision) = self.players.get(&player) else {
            return false;
        };
        (footprint.min.y..footprint.max.y).any(|y| {
            (footprint.min.x..footprint.max.x)
                .any(|x| vision.state(TileCoord::new(x, y)) == super::TileVisibility::Visible)
        })
    }
}

impl GameWorld {
    /// Tile index and watcher seeding for a new building footprint.
    pub(crate) fn index_new_building(
        &mut self,
        id: EntityId,
        player: PlayerId,
        footprint: TileRect,
    ) {
        for y in footprint.min.y..footprint.max.y {
            for x in footprint.min.x..footprint.max.x {
                self.vision
                    .building_at_tile
                    .insert(TileCoord::new(x, y), id);
            }
        }
        if self.vision.disabled {
            return;
        }
        let mut watchers = BTreeSet::from([player]);
        let players = self.vision.players.keys().copied().collect::<Vec<_>>();
        for seen in players {
            if self.building_visible_to(seen, footprint) {
                watchers.insert(seen);
            }
        }
        self.vision.building_watchers.insert(
            id,
            Watched {
                owner: player,
                footprint,
                watchers,
            },
        );
    }

    /// Full rebuild after fog returns: sight while disabled made no events.
    /// Buildings destroyed while fog was off stay queued for the flush.
    pub(crate) fn rebuild_building_sight_index(&mut self) {
        self.vision.building_at_tile.clear();
        self.vision.building_watchers.clear();
        let buildings = self
            .buildings()
            .map(|b| (b.id, b.player, b.footprint))
            .collect::<Vec<_>>();
        for (id, player, footprint) in buildings {
            self.index_new_building(id, player, footprint);
        }
    }

    /// Refreshes remembered snapshots for every watched building and drops
    /// the memory of destroyed ones whose site is now visible. Everything a
    /// player no longer sees keeps its last observation.
    pub(crate) fn remember_visible_buildings(&mut self) {
        if self.vision.disabled {
            return;
        }
        let watched = self
            .vision
            .building_watchers
            .iter()
            .map(|(id, seen)| (*id, seen.watchers.clone()))
            .collect::<Vec<_>>();
        for (id, watchers) in watched {
            let Some(building) = self.economy.buildings.get(&id).cloned() else {
                continue;
            };
            for player in watchers {
                if let Some(vision) = self.vision.players.get_mut(&player) {
                    // Only a differing snapshot is a change.
                    if vision.remembered.get(&id) != Some(&building) {
                        vision.changed.insert(id);
                        vision.remembered.insert(id, building.clone());
                    }
                }
            }
        }
    }

    /// Queues a removed building for the next flush.
    pub(crate) fn forget_building(&mut self, id: EntityId, footprint: TileRect) {
        for y in footprint.min.y..footprint.max.y {
            for x in footprint.min.x..footprint.max.x {
                self.vision.building_at_tile.remove(&TileCoord::new(x, y));
            }
        }
        let watched = self.vision.building_watchers.remove(&id);
        for (player, vision) in &mut self.vision.players {
            let Some(owner) = vision.remembered.get(&id).map(|b| b.player) else {
                continue;
            };
            let sees = watched
                .as_ref()
                .is_some_and(|w| w.watchers.contains(player));
            if sees || owner == *player {
                vision.remembered.remove(&id);
                vision.changed.insert(id);
            } else {
                vision.haunt(id, footprint);
            }
        }
    }
}

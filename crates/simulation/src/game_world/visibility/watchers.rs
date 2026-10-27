//! Building memory driven by sight events. A building's remembered snapshot
//! refreshes only for players currently seeing it. A destroyed building leaves
//! memory at once for its owner and its watchers; for everyone else it stays as
//! a ghost until a tile of its site is seen again.
use super::{PlayerVision, TileVisibility, Vision};
use crate::GameWorld;
use aoe_core::{EntityId, PlayerId, TileCoord, TileRect};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// The players currently seeing a live building. The owner is always one of
/// them, so owned destruction is known immediately.
#[derive(Debug)]
pub(super) struct Watched {
    pub(super) owner: PlayerId,
    pub(super) footprint: TileRect,
    pub(super) watchers: BTreeSet<PlayerId>,
}

/// One building-watch event of a sight change. Applied to the shared index
/// after the player's own tile work, in the order the events were produced.
#[derive(Clone, Copy, Debug)]
pub(super) enum Watch {
    /// The player now sees a tile of the building.
    Gain(EntityId),
    /// The player lost sight of a tile of a building they do not own. The
    /// watch ends when `hidden`: no tile of the footprint is visible any more.
    Loss { building: EntityId, hidden: bool },
}

/// Read-only building lookups used while the players are processed.
pub(super) struct WatchIndex<'a> {
    pub(super) building_at_tile: &'a HashMap<TileCoord, EntityId>,
    pub(super) watched: &'a BTreeMap<EntityId, Watched>,
}

impl PlayerVision {
    /// The player-local half of a sight change, judged on the sight at its
    /// end: ends the ghost memories of tiles seen again and collects the watch
    /// events of the flips, in flip order.
    pub(super) fn sight_events(
        &mut self,
        player: PlayerId,
        flips: &[(TileCoord, bool)],
        index: &WatchIndex,
        out: &mut Vec<Watch>,
    ) {
        for (tile, seen) in flips {
            if *seen && let Some(ids) = self.ghost_tiles.get(tile).cloned() {
                for id in ids {
                    self.exorcise(id);
                }
            }
            let Some(&building) = index.building_at_tile.get(tile) else {
                continue;
            };
            if *seen {
                out.push(Watch::Gain(building));
            } else if let Some(watched) = index.watched.get(&building)
                && watched.owner != player
            {
                out.push(Watch::Loss {
                    building,
                    hidden: !self.any_visible(watched.footprint),
                });
            }
        }
    }

    fn any_visible(&self, footprint: TileRect) -> bool {
        (footprint.min.y..footprint.max.y).any(|y| {
            (footprint.min.x..footprint.max.x)
                .any(|x| self.state(TileCoord::new(x, y)) == TileVisibility::Visible)
        })
    }
}

impl Vision {
    /// Applies one watch event of `player` to the shared index.
    pub(super) fn apply_watch(&mut self, player: PlayerId, watch: Watch) {
        match watch {
            Watch::Gain(building) => {
                if let Some(watched) = self.building_watchers.get_mut(&building) {
                    watched.watchers.insert(player);
                }
            }
            Watch::Loss { building, hidden } => {
                if hidden
                    && let Some(watched) = self.building_watchers.get_mut(&building)
                    && watched.watchers.contains(&player)
                {
                    watched.watchers.remove(&player);
                }
            }
        }
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
        let span = self.building(id).and_then(|b| b.bridge_span);
        for y in footprint.min.y..footprint.max.y {
            for x in footprint.min.x..footprint.max.x {
                if span.is_some_and(|span| !span.contains(TileCoord::new(x, y))) {
                    continue;
                }
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
                let tile = TileCoord::new(x, y);
                if self.vision.building_at_tile.get(&tile) == Some(&id) {
                    self.vision.building_at_tile.remove(&tile);
                }
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

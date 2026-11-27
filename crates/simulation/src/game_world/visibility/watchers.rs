//! Building memory driven by sight events. A building's remembered snapshot
//! refreshes only for players currently seeing it. A destroyed building leaves
//! memory at once for its owner and its watchers; for everyone else it stays as
//! a ghost until a tile of its site is seen again.
use super::{PlayerVision, TileVisibility, Vision};
use crate::GameWorld;
use aoe_core::{EntityId, PlayerId, TileCoord, TileRect};
use std::collections::{BTreeMap, BTreeSet};

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
    pub(super) building_at_tile: &'a super::TileBuildings,
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
                if let Some(watched) = self.building_watchers.get_mut(&building)
                    && watched.watchers.insert(player)
                {
                    self.dirty_buildings.insert(building);
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
    pub(crate) fn building_at_for(&self, player: PlayerId, tile: TileCoord) -> Option<EntityId> {
        self.building_at(tile).or_else(|| {
            self.vision
                .private_foundations
                .get(&(tile, player))
                .copied()
        })
    }

    pub(crate) fn tile_has_reservation(&self, tile: TileCoord) -> bool {
        self.building_at(tile).is_some()
            || self
                .vision
                .private_foundations
                .range((tile, PlayerId(0))..=(tile, PlayerId(u16::MAX)))
                .next()
                .is_some()
    }

    pub(crate) fn site_has_building(
        &self,
        footprint: TileRect,
        span: Option<aoe_core::BridgeSpan>,
        player: Option<PlayerId>,
    ) -> bool {
        footprint_tiles(footprint, span).any(|tile| {
            if let Some(player) = player {
                self.building_at_for(player, tile).is_some()
            } else {
                self.building_at(tile).is_some()
            }
        })
    }

    pub(crate) fn remove_foundation_reservation(
        &mut self,
        id: EntityId,
        player: PlayerId,
        footprint: TileRect,
        span: Option<aoe_core::BridgeSpan>,
    ) {
        for tile in footprint_tiles(footprint, span) {
            let key = (tile, player);
            if self.vision.private_foundations.get(&key) == Some(&id) {
                self.vision.private_foundations.remove(&key);
            }
        }
    }

    /// Tile index and watcher seeding for a new building footprint.
    pub(crate) fn index_new_building(
        &mut self,
        id: EntityId,
        player: PlayerId,
        footprint: TileRect,
    ) {
        let Some(building) = self.building(id) else {
            return;
        };
        let (private, span) = (building.is_unstarted(), building.bridge_span);
        if private {
            for tile in footprint_tiles(footprint, span) {
                self.vision.private_foundations.insert((tile, player), id);
            }
        } else {
            self.combat.quiet.wake_area(footprint);
            for tile in footprint_tiles(footprint, span) {
                if let Some(old) = self.vision.building_at_tile.insert(tile, id)
                    && let Some(owner) = self.economy.buildings.get(&old).map(|b| b.player)
                {
                    self.vision.building_chunks.remove_tile(tile, owner);
                    self.vision.building_cells.remove_cell_tile(tile, owner);
                }
                self.vision.building_chunks.add_tile(tile, player);
                self.vision.building_cells.add_cell_tile(tile, player);
            }
        }
        if self.vision.disabled {
            return;
        }
        self.vision.players.entry(player).or_default();
        self.vision.dirty_buildings.insert(id);
        let mut watchers = BTreeSet::from([player]);
        let mut asked = 0;
        if !private {
            // Counts with the question itself, so none is asked uncounted.
            let mut sees = |player| {
                asked += 1;
                self.building_visible_to(player, footprint)
            };
            for (&seen, vision) in &self.vision.players {
                // Without a visible tile nobody sees this building.
                if vision.visible_tiles == 0 {
                    continue;
                }
                if sees(seen) {
                    watchers.insert(seen);
                }
            }
        }
        self.vision.asked += asked;
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
        self.vision.building_chunks.clear();
        self.vision.building_cells.clear();
        self.vision.private_foundations.clear();
        self.vision.building_watchers.clear();
        let buildings = self
            .buildings()
            .map(|b| (b.id, b.player, b.footprint))
            .collect::<Vec<_>>();
        for (id, player, footprint) in buildings {
            self.index_new_building(id, player, footprint);
        }
    }

    /// Queues a removed building for the next flush.
    pub(crate) fn forget_building(&mut self, id: EntityId, player: PlayerId, footprint: TileRect) {
        self.combat.quiet.wake_area(footprint);
        self.vision.dirty_buildings.remove(&id);
        for y in footprint.min.y..footprint.max.y {
            for x in footprint.min.x..footprint.max.x {
                let tile = TileCoord::new(x, y);
                if self.vision.building_at_tile.get(&tile) == Some(&id) {
                    self.vision.building_at_tile.remove(&tile);
                    self.vision.building_chunks.remove_tile(tile, player);
                    self.vision.building_cells.remove_cell_tile(tile, player);
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

fn footprint_tiles(
    footprint: TileRect,
    span: Option<aoe_core::BridgeSpan>,
) -> impl Iterator<Item = TileCoord> {
    let rectangular = (footprint.min.y..footprint.max.y)
        .flat_map(move |y| (footprint.min.x..footprint.max.x).map(move |x| TileCoord::new(x, y)));
    let raster = span.into_iter().flat_map(|bridge| bridge.tiles());
    rectangular
        .take(if span.is_some() { 0 } else { usize::MAX })
        .chain(raster)
}

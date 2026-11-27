//! Building memory driven by sight events. A building's remembered snapshot
//! refreshes only for players currently seeing it. A destroyed building leaves
//! memory at once for its owner and its watchers; for everyone else it stays as
//! a ghost until a tile of its site is seen again.
use super::{PlayerVision, TileVisibility, Vision};
use crate::{GameBuilding, GameWorld};
use aoe_core::{EntityId, PlayerId, TileCoord, TileRect};
use std::borrow::Cow;
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
            (footprint.min.x..footprint.max.x).any(|x| {
                #[cfg(test)]
                PROBES.with(|probes| probes.set(probes.get() + 1));
                self.state(TileCoord::new(x, y)) == TileVisibility::Visible
            })
        })
    }

    /// This player's memory, with the `live` building in its place where the
    /// player sees it and memory may be behind. A player is a watcher of every
    /// building it sees, unless the building is `part_indexed`; a refresh left
    /// each watcher's memory equal to the live building, and what changed one
    /// since marked it dirty. `behind` names both kinds. Only work in progress
    /// moves without a mark: it runs on a clock, and a tech rescales it.
    fn known<'a>(
        &self,
        player: PlayerId,
        behind: impl Iterator<Item = EntityId>,
        live: impl Fn(EntityId) -> Option<Cow<'a, GameBuilding>>,
    ) -> Vec<GameBuilding> {
        let in_sight = |id: EntityId| {
            live(id).filter(|b| {
                b.player == player || (!b.is_unstarted() && self.any_visible(b.footprint))
            })
        };
        let mut known = self
            .remembered
            .iter()
            .map(|(id, last)| {
                let working = !last.is_complete()
                    || !last.training_queue.is_empty()
                    || last.research.is_some();
                let now = if working { in_sight(*id) } else { None };
                now.map_or_else(|| last.clone(), Cow::into_owned)
            })
            .collect::<Vec<_>>();
        let mut unremembered = Vec::new();
        for building in behind.filter_map(in_sight) {
            let building = building.into_owned();
            match known.binary_search_by_key(&building.id, |b| b.id) {
                Ok(at) => known[at] = building,
                Err(_) => unremembered.push(building),
            }
        }
        if !unremembered.is_empty() {
            known.append(&mut unremembered);
            known.sort_unstable_by_key(|b| b.id);
            // One building may be behind for both reasons.
            known.dedup_by_key(|b| b.id);
        }
        known
    }
}

#[cfg(test)]
thread_local! {
    /// Footprint tiles asked for sight on this thread.
    static PROBES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
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
    /// Authoritative last observations, including buildings destroyed unseen.
    pub fn known_buildings(&self, player: PlayerId) -> Vec<GameBuilding> {
        if self.vision.disabled {
            return self
                .buildings()
                .filter(|b| !b.is_unstarted() || b.player == player)
                .map(Cow::into_owned)
                .collect();
        }
        let known = self.known_from_memory(player);
        #[cfg(test)]
        assert_eq!(known, self.scan_known_buildings(player), "{player:?}");
        known
    }

    /// `known_buildings` with fog on. The player's sight is all it may walk;
    /// the world answers for one building at a time.
    pub(crate) fn known_from_memory(&self, player: PlayerId) -> Vec<GameBuilding> {
        let Some(vision) = self.vision.players.get(&player) else {
            // An owner or a seat that ever saw a tile has one.
            return Vec::new();
        };
        let behind = self
            .vision
            .dirty_buildings
            .iter()
            .chain(&self.vision.part_indexed)
            .copied();
        vision.known(player, behind, |id| self.building(id))
    }

    /// Footprint tiles asked for sight on this thread so far.
    #[cfg(test)]
    pub(crate) fn footprint_probes(&self) -> usize {
        PROBES.with(std::cell::Cell::get)
    }

    /// Test oracle: what a scan of every building in the world finds known.
    #[cfg(test)]
    fn scan_known_buildings(&self, player: PlayerId) -> Vec<GameBuilding> {
        let mut known = self
            .vision
            .players
            .get(&player)
            .map(|v| v.remembered.clone())
            .unwrap_or_default();
        for building in self.buildings().filter(|b| {
            b.player == player
                || (!b.is_unstarted() && self.building_visible_to(player, b.footprint))
        }) {
            known.insert(building.id, building.into_owned());
        }
        known.into_values().collect()
    }

    pub(crate) fn building_at_for(&self, player: PlayerId, tile: TileCoord) -> Option<EntityId> {
        self.building_at(tile).or_else(|| {
            self.vision
                .private_foundations
                .get(&(player, tile))
                .copied()
        })
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
            let key = (player, tile);
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
                self.vision.private_foundations.insert((player, tile), id);
            }
        } else {
            self.combat.quiet.wake_area(footprint);
            let mut unindexed = i64::from(footprint.width()) * i64::from(footprint.height());
            for tile in footprint_tiles(footprint, span) {
                unindexed -= 1;
                if let Some(old) = self.vision.building_at_tile.insert(tile, id)
                    && let Some(owner) = self.economy.buildings.get(&old).map(|b| b.player)
                {
                    self.vision.part_indexed.insert(old);
                    self.vision.building_chunks.remove_tile(tile, owner);
                    self.vision.building_cells.remove_cell_tile(tile, owner);
                }
                self.vision.building_chunks.add_tile(tile, player);
                self.vision.building_cells.add_cell_tile(tile, player);
            }
            if unindexed > 0 {
                self.vision.part_indexed.insert(id);
            }
        }
        if self.vision.disabled {
            return;
        }
        self.vision.players.entry(player).or_default();
        self.vision.dirty_buildings.insert(id);
        let mut watchers = BTreeSet::from([player]);
        if !private {
            for &seen in self.vision.players.keys() {
                if self.building_visible_to(seen, footprint) {
                    watchers.insert(seen);
                }
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
        self.vision.building_chunks.clear();
        self.vision.building_cells.clear();
        self.vision.private_foundations.clear();
        self.vision.part_indexed.clear();
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
        self.vision.part_indexed.remove(&id);
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

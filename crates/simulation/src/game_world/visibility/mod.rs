//! Sparse player knowledge. Sight spreads through full-disc updates when an
//! observer appears, dies or changes LOS, and through disc-edge diffs when it
//! merely moves. Building memory consumes the sight flips separately. No
//! allocation or iteration depends on map area.
use super::{GameBuilding, GameUnit, GameWorld, UnitKind};
use aoe_core::{EntityId, PlayerId, TileCoord, TileRect};
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TileVisibility {
    Unexplored,
    Explored,
    Visible,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Observer {
    player: PlayerId,
    center: TileCoord,
    los: u8,
}

#[derive(Default, Debug)]
pub(crate) struct Vision {
    players: BTreeMap<PlayerId, PlayerVision>,
    observers: observers::Observers,
    /// Building footprint tiles, for sight-gain and sight-loss events.
    building_at_tile: HashMap<TileCoord, EntityId>,
    building_watchers: BTreeMap<EntityId, Watched>,
    disabled: bool,
    revision: u64,
    /// While a movement tick runs, observer changes queue here and apply per
    /// player in parallel afterwards (see `batch.rs`).
    batch: Option<Vec<change::Change>>,
}

mod batch;
mod change;
mod observers;

#[path = "state.rs"]
mod state;
use state::PlayerVision;

#[path = "watchers.rs"]
mod watchers;
use watchers::Watched;

impl GameWorld {
    pub fn visibility(&self, player: PlayerId, tile: TileCoord) -> TileVisibility {
        if self.vision.disabled {
            return TileVisibility::Visible;
        }
        self.vision
            .players
            .get(&player)
            .map_or(TileVisibility::Unexplored, |v| v.state(tile))
    }

    /// Scenario switch; clients cannot grant themselves knowledge with a URL.
    pub fn set_fog_disabled(&mut self, disabled: bool) {
        let was_disabled = self.vision.disabled;
        self.vision.disabled = disabled;
        self.vision.revision += 1;
        if was_disabled && !disabled {
            self.vision.observers.clear();
            for vision in self.vision.players.values_mut() {
                vision.revision += 1;
                vision.visible.clear();
                vision.visible_tiles = 0;
                vision.overview.clear_counts();
            }
            self.rebuild_building_sight_index();
            let units = self.units().map(|u| u.id).collect::<Vec<_>>();
            let buildings = self.buildings().map(|b| b.id).collect::<Vec<_>>();
            for id in units {
                self.update_unit_vision(id);
            }
            for id in buildings {
                self.update_building_vision(id);
            }
            let animals = self
                .animals()
                .filter(|a| a.hp > 0)
                .filter_map(|a| {
                    a.owner
                        .map(|p| (a.id, p, a.position.tile_floor(), a.kind.rules().los_tiles))
                })
                .collect::<Vec<_>>();
            for (id, p, tile, los) in animals {
                self.update_observer(id, Some((p, tile, los)));
            }
            self.remember_visible_buildings();
        }
    }
    pub fn fog_disabled(&self) -> bool {
        self.vision.disabled
    }
    pub fn vision_revision(&self) -> u64 {
        self.vision.revision
    }
    /// Changes only when `player`'s own sight changes, so a viewer can skip
    /// rebuilding its fog view when other players move. With fog disabled it
    /// follows the global revision.
    pub fn player_vision_revision(&self, player: PlayerId) -> u64 {
        if self.vision.disabled {
            return self.vision.revision;
        }
        self.vision.players.get(&player).map_or(0, |v| v.revision)
    }
    pub fn vision_overview(&self, player: PlayerId) -> Vec<(u16, u8)> {
        self.vision
            .players
            .get(&player)
            .map_or_else(Vec::new, |v| v.overview.states().collect())
    }

    pub fn unit_visible_to(&self, player: PlayerId, id: EntityId) -> bool {
        self.unit(id).is_some_and(|u| {
            u.player == player
                || self.visibility(player, u.position.tile_floor()) == TileVisibility::Visible
        })
    }

    pub fn building_visible_to(&self, player: PlayerId, footprint: TileRect) -> bool {
        (footprint.min.y..footprint.max.y).any(|y| {
            (footprint.min.x..footprint.max.x)
                .any(|x| self.visibility(player, TileCoord::new(x, y)) == TileVisibility::Visible)
        })
    }

    /// Authoritative last observations, including buildings destroyed unseen.
    pub fn known_buildings(&self, player: PlayerId) -> Vec<GameBuilding> {
        if self.vision.disabled {
            return self.buildings().cloned().collect();
        }
        let mut known = self
            .vision
            .players
            .get(&player)
            .map(|v| v.remembered.clone())
            .unwrap_or_default();
        for building in self
            .buildings()
            .filter(|b| b.player == player || self.building_visible_to(player, b.footprint))
        {
            known.insert(building.id, building.clone());
        }
        known.into_values().collect()
    }

    /// Chunk count, current visible tile count and total tile touches, for
    /// bounded-cost regressions. Idle ticks perform no LOS tile work.
    pub fn vision_storage(&self, player: PlayerId) -> (usize, usize, u64) {
        self.vision.players.get(&player).map_or((0, 0, 0), |v| {
            (v.explored.len(), v.visible_tiles, v.tile_work)
        })
    }

    pub(crate) fn update_observer(
        &mut self,
        id: EntityId,
        observer: Option<(PlayerId, TileCoord, u8)>,
    ) -> bool {
        if self.vision.disabled {
            return false;
        }
        let observer = observer.map(|(player, center, los)| Observer {
            player,
            center,
            los,
        });
        if self.vision.observers.get(&id) == observer {
            return false;
        }
        self.vision.revision += 1;
        let old = self.vision.observers.remove(&id);
        let changes = match (old, observer) {
            (Some(old), Some(new)) if old.player == new.player && old.los == new.los => {
                vec![change::Change::Edge { old, new }]
            }
            (old, new) => old
                .map(|observer| change::Change::Disc {
                    observer,
                    add: false,
                })
                .into_iter()
                .chain(new.map(|observer| change::Change::Disc {
                    observer,
                    add: true,
                }))
                .collect(),
        };
        if let Some(new) = observer {
            self.vision.observers.insert(id, new);
        }
        self.submit_vision_changes(changes);
        true
    }

    /// What a unit currently observes: owner, tile and line of sight. Pure, so
    /// the parallel movement proposal can compute it from a snapshot.
    pub(crate) fn unit_observer(&self, u: &GameUnit) -> Option<(PlayerId, TileCoord, u8)> {
        Self::unit_observer_with(u, self.player_economy(u.player))
    }

    /// `unit_observer` for an already fetched owner economy.
    pub(crate) fn unit_observer_with(
        u: &GameUnit,
        economy: Option<super::PlayerEconomy>,
    ) -> Option<(PlayerId, TileCoord, u8)> {
        if u.hp == 0 {
            return None;
        }
        let researched = economy.map_or(0, |p| p.researched);
        let los = match u.kind {
            UnitKind::Scout => {
                if researched & super::Tech::FeudalAge.bit() != 0 {
                    6
                } else {
                    4
                }
            }
            _ => {
                super::combat::military::researched_stats(
                    u.kind,
                    economy.is_some_and(|p| p.age == super::Age::Feudal),
                    researched,
                )
                .los
            }
        };
        Some((u.player, u.position.tile_floor(), los))
    }

    /// True when `update_unit_vision` would change the shared vision state for
    /// a unit in this state; false means it is a no-op and may be skipped.
    pub(crate) fn unit_vision_stale(
        &self,
        u: &GameUnit,
        economy: Option<super::PlayerEconomy>,
    ) -> bool {
        if self.vision.disabled {
            return false;
        }
        let observer = Self::unit_observer_with(u, economy).map(|(player, center, los)| Observer {
            player,
            center,
            los,
        });
        self.vision.observers.get(&u.id) != observer
    }

    pub(crate) fn update_unit_vision(&mut self, id: EntityId) {
        let observer = self.unit(id).and_then(|u| self.unit_observer(&u));
        self.update_observer(id, observer);
    }

    pub(crate) fn update_building_vision(&mut self, id: EntityId) {
        let observer = self.building(id).map(|b| {
            (
                b.player,
                TileCoord::new(
                    (b.footprint.min.x + b.footprint.max.x) / 2,
                    (b.footprint.min.y + b.footprint.max.y) / 2,
                ),
                b.los,
            )
        });
        self.update_observer(id, observer);
    }
    /// What `player` remembers of a building: its last observation.
    pub fn remembered_building(&self, player: PlayerId, id: EntityId) -> Option<&GameBuilding> {
        self.vision.players.get(&player)?.remembered.get(&id)
    }

    /// Takes, per player, the buildings whose memory changed since the last
    /// call, in id order. A consumer that tracks `remembered_building` per
    /// player needs to look at no other building.
    pub fn take_remembered_building_changes(&mut self) -> BTreeMap<PlayerId, BTreeSet<EntityId>> {
        self.vision
            .players
            .iter_mut()
            .filter(|(_, vision)| !vision.changed.is_empty())
            .map(|(player, vision)| (*player, std::mem::take(&mut vision.changed)))
            .collect()
    }

    /// `player`'s knowledge of every tile in `rect`, row-major: 0 unexplored,
    /// 1 explored, 2 visible; the same answers as `visibility` per tile
    /// without a map lookup per tile. A world with fog disabled sees all.
    pub fn visibility_tiles(&self, player: PlayerId, rect: TileRect) -> Vec<u8> {
        let width = rect.width().max(0) as usize;
        let mut tiles = vec![0; width * rect.height().max(0) as usize];
        let Some(vision) = self
            .vision
            .players
            .get(&player)
            .filter(|_| !tiles.is_empty())
        else {
            return tiles;
        };
        let (first, last) = (
            (rect.min.x.div_euclid(32), rect.min.y.div_euclid(32)),
            (
                (rect.max.x - 1).div_euclid(32),
                (rect.max.y - 1).div_euclid(32),
            ),
        );
        for chunk_y in first.1..=last.1 {
            for chunk_x in first.0..=last.0 {
                let Some(bits) = vision.explored.get(&(chunk_x, chunk_y)) else {
                    continue;
                };
                for y in rect.min.y.max(chunk_y * 32)..rect.max.y.min(chunk_y * 32 + 32) {
                    for x in rect.min.x.max(chunk_x * 32)..rect.max.x.min(chunk_x * 32 + 32) {
                        let bit = ((y - chunk_y * 32) * 32 + (x - chunk_x * 32)) as usize;
                        if bits[bit / 64] & (1 << (bit % 64)) != 0 {
                            tiles[(y - rect.min.y) as usize * width + (x - rect.min.x) as usize] =
                                1;
                        }
                    }
                }
            }
        }
        for chunk_y in first.1..=last.1 {
            for chunk_x in first.0..=last.0 {
                let Some(counters) = vision.visible.get(&(chunk_x, chunk_y)) else {
                    continue;
                };
                for y in rect.min.y.max(chunk_y * 32)..rect.max.y.min(chunk_y * 32 + 32) {
                    for x in rect.min.x.max(chunk_x * 32)..rect.max.x.min(chunk_x * 32 + 32) {
                        if counters[((y - chunk_y * 32) * 32 + (x - chunk_x * 32)) as usize] > 0 {
                            tiles[(y - rect.min.y) as usize * width + (x - rect.min.x) as usize] =
                                2;
                        }
                    }
                }
            }
        }
        tiles
    }
}

#[path = "hash.rs"]
mod hash;
mod resources;

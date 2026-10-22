//! Sparse player knowledge. Sight spreads through full-disc updates when an
//! observer appears, dies or changes LOS, and through disc-edge diffs when it
//! merely moves. Building memory consumes the sight flips separately.
use super::{GameBuilding, GameWorld, UnitKind};
use aoe_core::{EntityId, PlayerId, TileCoord, TileRect};
use std::collections::{BTreeMap, HashMap};

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
    observers: BTreeMap<EntityId, Observer>,
    /// Building footprint tiles, for sight-gain and sight-loss events.
    building_at_tile: HashMap<TileCoord, EntityId>,
    building_watchers: BTreeMap<EntityId, Watched>,
    /// Scratch for disc-edge diffs; working memory, never hashed.
    flags: Vec<bool>,
    flips: Vec<(TileCoord, bool)>,
    entered: Vec<TileCoord>,
    disabled: bool,
    revision: u64,
}

#[path = "state.rs"]
mod state;
use state::PlayerVision;

#[path = "watchers.rs"]
mod watchers;
use watchers::Watched;

impl Vision {
    /// Full disc update: a fresh observer or one whose LOS changed.
    fn apply(&mut self, observer: Observer, add: bool, width: i32, height: i32) {
        let player = self.players.entry(observer.player).or_default();
        player.revision += 1;
        let radius = i32::from(observer.los);
        let mut flips = std::mem::take(&mut self.flips);
        flips.clear();
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx * dx + dy * dy > radius * radius {
                    continue;
                }
                let tile = TileCoord::new(observer.center.x + dx, observer.center.y + dy);
                if tile.x < 0 || tile.y < 0 || tile.x >= width || tile.y >= height {
                    continue;
                }
                player.tile_work += 1;
                let flipped = if add {
                    player.raise_visible(tile)
                } else {
                    player.lower_visible(tile)
                };
                if !flipped {
                    continue;
                }
                if add {
                    player.set_explored(tile);
                }
                player.touch_overview(tile, add, width, height);
                flips.push((tile, add));
            }
        }
        self.flips = flips;
        let mut entered = std::mem::take(&mut self.entered);
        entered.extend(
            self.flips
                .iter()
                .filter(|(_, add)| *add)
                .map(|(tile, _)| *tile),
        );
        self.entered = entered;
        self.sight_flips(observer.player);
    }

    /// Disc-edge update: only the entering and leaving tiles of a moved
    /// observer touch counters, exploration, overview and building memory.
    /// Flag slots are offsets from the old center, so both discs share them.
    fn apply_edge(&mut self, old: Observer, new: Observer, width: i32, height: i32) {
        let radius = i32::from(old.los);
        let span = (2 * radius + 1) as usize;
        let mut flags = std::mem::take(&mut self.flags);
        flags.clear();
        flags.resize(span * span, false);
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx * dx + dy * dy > radius * radius {
                    continue;
                }
                let tile = TileCoord::new(old.center.x + dx, old.center.y + dy);
                if tile.x >= 0 && tile.y >= 0 && tile.x < width && tile.y < height {
                    flags[((dy + radius) as usize) * span + (dx + radius) as usize] = true;
                }
            }
        }
        let (shift_x, shift_y) = (new.center.x - old.center.x, new.center.y - old.center.y);
        let player = self.players.entry(new.player).or_default();
        player.revision += 1;
        let mut flips = std::mem::take(&mut self.flips);
        flips.clear();
        let mut entered = std::mem::take(&mut self.entered);
        entered.clear();
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx * dx + dy * dy > radius * radius {
                    continue;
                }
                let tile = TileCoord::new(new.center.x + dx, new.center.y + dy);
                if tile.x < 0 || tile.y < 0 || tile.x >= width || tile.y >= height {
                    continue;
                }
                // The same tile in old-disc coordinates.
                let (ox, oy) = (shift_x + dx, shift_y + dy);
                if ox.abs() <= radius
                    && oy.abs() <= radius
                    && flags[((oy + radius) as usize) * span + (ox + radius) as usize]
                {
                    flags[((oy + radius) as usize) * span + (ox + radius) as usize] = false;
                    continue;
                }
                player.tile_work += 1;
                if player.raise_visible(tile) {
                    player.set_explored(tile);
                    flips.push((tile, true));
                    entered.push(tile);
                    player.touch_overview(tile, true, width, height);
                }
            }
        }
        for (slot, leaving) in flags.iter().enumerate() {
            if !leaving {
                continue;
            }
            let (dy, dx) = ((slot / span) as i32 - radius, (slot % span) as i32 - radius);
            let tile = TileCoord::new(old.center.x + dx, old.center.y + dy);
            player.tile_work += 1;
            if player.lower_visible(tile) {
                flips.push((tile, false));
                player.touch_overview(tile, false, width, height);
            }
        }
        self.flags = flags;
        self.flips = flips;
        self.entered = entered;
        self.sight_flips(new.player);
    }
}

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
        if self.vision.observers.get(&id).copied() == observer {
            return false;
        }
        self.vision.revision += 1;
        let old = self.vision.observers.remove(&id);
        let (width, height) = (self.config.width_tiles, self.config.height_tiles);
        match (old, observer) {
            (Some(old), Some(new)) if old.player == new.player && old.los == new.los => {
                self.vision.apply_edge(old, new, width, height)
            }
            (old, new) => {
                if let Some(old) = old {
                    self.vision.apply(old, false, width, height);
                }
                if let Some(new) = new {
                    self.vision.apply(new, true, width, height);
                }
            }
        };
        let entered = std::mem::take(&mut self.vision.entered);
        if let Some(new) = observer {
            self.vision.observers.insert(id, new);
            self.observe_resource_tiles(new.player, &entered);
        }
        true
    }

    pub(crate) fn update_unit_vision(&mut self, id: EntityId) {
        let observer = self.unit(id).filter(|u| u.hp > 0).map(|u| {
            let los = match u.kind {
                UnitKind::Scout => {
                    if self.has_researched(u.player, super::Tech::FeudalAge) {
                        6
                    } else {
                        4
                    }
                }
                _ => self
                    .unit_stats(u.id)
                    .map_or(super::unit_combat(u.kind).los, |s| s.los),
            };
            (u.player, u.position.tile_floor(), los)
        });
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
}

#[path = "hash.rs"]
mod hash;
mod resources;

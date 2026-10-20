//! Sparse player knowledge. No allocation or iteration depends on map area.
use super::{GameBuilding, GameWorld, UnitKind};
use aoe_core::{EntityId, PlayerId, TileCoord, TileRect};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TileVisibility {
    Unexplored,
    Explored,
    Visible,
}

#[derive(Default, Debug)]
struct PlayerVision {
    // 32x32 bits, allocated only after a tile in the chunk was observed.
    explored: BTreeMap<(i32, i32), [u64; 16]>,
    // Only currently visible tiles carry reference counts. Overlapping LOS
    // cannot turn a tile fogged when just one observer leaves.
    visible: BTreeMap<TileCoord, u32>,
    remembered: BTreeMap<EntityId, GameBuilding>,
    tile_work: u64,
    overview: BTreeMap<u16, u32>,
    resources: BTreeMap<u64, u16>,
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
    disabled: bool,
    revision: u64,
}

fn address(tile: TileCoord) -> ((i32, i32), usize, u64) {
    let index = (tile.y.rem_euclid(32) * 32 + tile.x.rem_euclid(32)) as usize;
    (
        (tile.x.div_euclid(32), tile.y.div_euclid(32)),
        index / 64,
        1 << (index % 64),
    )
}

/// Every strategic pixel intersecting the tile, including tiles wider than a
/// pixel on supported small maps. Shared pixels retain reference counts.
fn pixels(tile: i32, axis: i32) -> std::ops::Range<i64> {
    let (tile, axis) = (i64::from(tile), i64::from(axis));
    tile * 256 / axis..((tile + 1) * 256 + axis - 1) / axis
}

impl PlayerVision {
    fn state(&self, tile: TileCoord) -> TileVisibility {
        if self.visible.contains_key(&tile) {
            return TileVisibility::Visible;
        }
        let (chunk, word, mask) = address(tile);
        if self
            .explored
            .get(&chunk)
            .is_some_and(|bits| bits[word] & mask != 0)
        {
            TileVisibility::Explored
        } else {
            TileVisibility::Unexplored
        }
    }
}

impl Vision {
    fn apply(&mut self, observer: Observer, add: bool, width: i32, height: i32) {
        let player = self.players.entry(observer.player).or_default();
        let radius = i32::from(observer.los);
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
                if add {
                    *player.visible.entry(tile).or_default() += 1;
                    let (chunk, word, mask) = address(tile);
                    player.explored.entry(chunk).or_insert([0; 16])[word] |= mask;
                } else if let Some(count) = player.visible.get_mut(&tile) {
                    *count -= 1;
                    if *count == 0 {
                        player.visible.remove(&tile);
                    }
                } else {
                    continue;
                }
                for y in pixels(tile.y, height) {
                    for x in pixels(tile.x, width) {
                        let pixel = (y * 256 + x) as u16;
                        if add {
                            *player.overview.entry(pixel).or_default() += 1;
                        } else if let Some(total) = player.overview.get_mut(&pixel) {
                            *total -= 1;
                        }
                    }
                }
            }
        }
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
                vision.visible.clear();
                for count in vision.overview.values_mut() {
                    *count = 0;
                }
            }
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
    pub fn vision_overview(&self, player: PlayerId) -> Vec<(u16, u8)> {
        self.vision.players.get(&player).map_or_else(Vec::new, |v| {
            v.overview
                .iter()
                .map(|(i, count)| (*i, if *count > 0 { 2 } else { 1 }))
                .collect()
        })
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
            (v.explored.len(), v.visible.len(), v.tile_work)
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
        if let Some(old) = self.vision.observers.remove(&id) {
            self.vision.apply(
                old,
                false,
                self.config.width_tiles,
                self.config.height_tiles,
            );
        }
        if let Some(new) = observer {
            self.vision
                .apply(new, true, self.config.width_tiles, self.config.height_tiles);
            self.vision.observers.insert(id, new);
            self.observe_resource_disk(new);
        }
        true
    }

    pub(crate) fn update_unit_vision(&mut self, id: EntityId) {
        let observer = self.unit(id).filter(|u| u.hp > 0).map(|u| {
            let los = match u.kind {
                UnitKind::Villager | UnitKind::Militia => 4,
                UnitKind::Scout => {
                    if self.has_researched(u.player, super::Tech::FeudalAge) {
                        6
                    } else {
                        4
                    }
                }
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

    pub(crate) fn remember_visible_buildings(&mut self) {
        if self.vision.disabled {
            return;
        }
        let players = self.vision.players.keys().copied().collect::<Vec<_>>();
        for player in players {
            let visible = self
                .buildings()
                .filter(|b| b.player == player || self.building_visible_to(player, b.footprint))
                .cloned()
                .collect::<Vec<_>>();
            let vanished = self.vision.players[&player]
                .remembered
                .values()
                .filter(|b| {
                    !self.economy.buildings.contains_key(&b.id)
                        && (b.player == player || self.building_visible_to(player, b.footprint))
                })
                .map(|b| b.id)
                .collect::<Vec<_>>();
            let Some(vision) = self.vision.players.get_mut(&player) else {
                continue;
            };
            let memory = &mut vision.remembered;
            for id in vanished {
                memory.remove(&id);
            }
            for building in visible {
                memory.insert(building.id, building);
            }
        }
    }
}

#[path = "hash.rs"]
mod hash;
mod resources;

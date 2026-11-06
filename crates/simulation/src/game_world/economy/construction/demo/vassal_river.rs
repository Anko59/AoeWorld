//! Real-map Vassal fixture: a castle on one bank of a river, a Town Center on
//! the other. Everything after setup (planning, bridge, roads, walls) is the
//! Vassal's ordinary work.
use crate::{BuildingKind, EconomyError, GameWorld, Stockpile, Terrain, UnitKind};
use aoe_core::{BridgeSpan, EntityId, PlayerId, TileCoord, TileRect, WorldPosition};
use aoe_map::WaterKind;

fn position(tile: TileCoord) -> Result<WorldPosition, EconomyError> {
    WorldPosition::from_tile_center(tile).map_err(|_| EconomyError::InvalidFootprint)
}

impl GameWorld {
    fn vassal_river_land(&self, tile: TileCoord) -> Option<bool> {
        let Terrain::Map { generator, overlay } = &self.terrain else {
            return None;
        };
        let (sample, node) = generator
            .tile_and_node_with_cancel(tile, &|| false)
            .ok()??;
        if overlay.blocks_tile(tile) || node.is_some_and(|n| overlay.blocks_node(n)) {
            return None;
        }
        if sample.water == WaterKind::River {
            Some(false)
        } else if sample.water == WaterKind::None && sample.passable {
            Some(true)
        } else {
            None
        }
    }

    /// First placeable `kind` footprint near `at`.
    fn vassal_river_spot(&self, kind: BuildingKind, at: TileCoord) -> Option<TileCoord> {
        (0..=8_i32).find_map(|r| {
            (-r..=r)
                .flat_map(|dy| (-r..=r).map(move |dx| (dx, dy)))
                .filter(|(dx, dy)| dx.abs() == r || dy.abs() == r)
                .map(|(dx, dy)| TileCoord::new(at.x + dx, at.y + dy))
                .find(|t| self.validate_building_placement(kind, *t).is_ok())
        })
    }

    /// Off-tick setup on the loaded map: the nearest river to the map centre
    /// narrow enough for a straight bridge, a castle 20 tiles from the west
    /// (or north) bank with its Vassal, builders and soldiers, and a Town
    /// Center 24 tiles beyond the far bank. Returns the Vassal and the span
    /// a road across the river must use.
    pub fn prepare_vassal_river_demo(&mut self) -> Result<(EntityId, BridgeSpan), EconomyError> {
        let side = self.config.width_tiles.min(self.config.height_tiles);
        let (low, high) = ((side / 2 - 256).max(40), (side / 2 + 256).min(side - 40));
        let mut origins: Vec<_> = (low..high)
            .step_by(3)
            .flat_map(|y| (low..high).step_by(3).map(move |x| TileCoord::new(x, y)))
            .collect();
        origins.sort_unstable_by_key(|t| {
            ((t.x - side / 2).pow(2) + (t.y - side / 2).pow(2), t.y, t.x)
        });
        let found = origins
            .into_iter()
            .take(30_000)
            .find_map(|at| {
                if self.vassal_river_land(at) != Some(false) {
                    return None;
                }
                for (dx, dy) in [(1, 0), (0, 1)] {
                    let bank = |sign: i32| {
                        (1..=14)
                            .map(|n| TileCoord::new(at.x + sign * dx * n, at.y + sign * dy * n))
                            .find(|&t| self.vassal_river_land(t) == Some(true))
                    };
                    let (Some(from), Some(to)) = (bank(-1), bank(1)) else {
                        continue;
                    };
                    let span = BridgeSpan::new(from, to)?;
                    if !span.tiles().into_iter().all(|t| {
                        (self.vassal_river_land(t) == Some(true)) == (t == from || t == to)
                    }) || !self.bridge_site_valid(from, to)
                    {
                        continue;
                    }
                    let castle = self.vassal_river_spot(
                        BuildingKind::Castle,
                        TileCoord::new(from.x - dx * 20, from.y - dy * 20),
                    );
                    let town = self.vassal_river_spot(
                        BuildingKind::TownCenter,
                        TileCoord::new(to.x + dx * 24, to.y + dy * 24),
                    );
                    if let (Some(castle), Some(town)) = (castle, town) {
                        return Some((span, castle, town));
                    }
                }
                None
            })
            .ok_or(EconomyError::InvalidFootprint)?;
        let (span, castle_at, town_at) = found;
        for id in self.units().map(|u| u.id).collect::<Vec<_>>() {
            if let Some(&index) = self.lookup.get(&id)
                && self.units[index].state.hp > 0
            {
                self.kill_unit(index);
            }
            self.remove_unit(id);
        }
        self.remove_animals(crate::AnimalKind::Wolf);
        self.config.tick_hz = 5;
        self.set_fog_disabled(true);
        self.place_town_center(PlayerId(0), town_at)?;
        self.complete_research(PlayerId(0), crate::Tech::FeudalAge, &mut Vec::new());
        self.complete_research(PlayerId(0), crate::Tech::CastleAge, &mut Vec::new());
        if let Some(p) = self.economy.players.get_mut(&PlayerId(0)) {
            p.stockpile = Stockpile {
                food: 20_000,
                wood: 20_000,
                gold: 20_000,
                stone: 20_000,
            };
        }
        let castle = self.insert_building(
            PlayerId(0),
            BuildingKind::Castle,
            TileRect::from_xywh(castle_at.x, castle_at.y, 4, 4),
            true,
        )?;
        let near = |n: i32| TileCoord::new(castle_at.x - 3 + n % 6, castle_at.y + 6 + n / 6);
        let mut spawned = 0;
        for n in 0..40 {
            if spawned == 14 {
                break;
            }
            if self.spawn_villager(PlayerId(0), position(near(n))?).is_ok() {
                spawned += 1;
            }
        }
        for n in 0..6 {
            let _ = self.spawn_kind(
                PlayerId(0),
                UnitKind::Militia,
                position(TileCoord::new(castle_at.x + 5, castle_at.y + n))?,
            );
        }
        let vassal = self.spawn_kind(
            PlayerId(0),
            UnitKind::Vassal,
            position(TileCoord::new(castle_at.x - 1, castle_at.y + 1))?,
        )?;
        self.issue_garrison(PlayerId(0), vassal, castle)?;
        Ok((vassal, span))
    }
}

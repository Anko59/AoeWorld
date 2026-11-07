//! Original map terrain with a local recording army and a completed bridge.
use crate::{BuildingKind, EconomyError, GameWorld, Terrain, UnitKind};
use aoe_core::{BridgeSpan, EntityId, PlayerId, TileCoord, WorldPosition};
use aoe_map::WaterKind;

fn position(tile: TileCoord) -> Result<WorldPosition, EconomyError> {
    WorldPosition::from_tile_center(tile).map_err(|_| EconomyError::InvalidFootprint)
}

impl GameWorld {
    /// Off-tick local showcase setup. It samples the existing map, never
    /// replaces terrain pages, and leaves the march for an ordinary UI order.
    /// At most 16,384 sample origins, two diagonal axes and 24 tiles per half-span.
    pub fn prepare_general_river_demo(
        &mut self,
    ) -> Result<(EntityId, BridgeSpan, WorldPosition), EconomyError> {
        let side = self.config.width_tiles.min(self.config.height_tiles);
        let low = (side / 2 - 256).max(8);
        let high = (side / 2 + 256).min(side - 8);
        let mut origins: Vec<_> = (low..high)
            .step_by(4)
            .flat_map(|y| (low..high).step_by(4).map(move |x| TileCoord::new(x, y)))
            .collect();
        origins.sort_unstable_by_key(|t| {
            ((t.x - side / 2).pow(2) + (t.y - side / 2).pow(2), t.y, t.x)
        });
        let found = origins
            .into_iter()
            .take(16_384)
            .find_map(|at| {
                if self.demo_water(at) != Some(true) {
                    return None;
                }
                for (dx, dy) in [(1, 1), (1, -1)] {
                    let bank = |sign| {
                        (1..=24)
                            .map(|n| TileCoord::new(at.x + sign * dx * n, at.y + sign * dy * n))
                            .find(|&t| self.demo_water(t) == Some(false))
                    };
                    let (Some(from), Some(to)) = (bank(-1), bank(1)) else {
                        continue;
                    };
                    let Some(span) = BridgeSpan::new(from, to) else {
                        continue;
                    };
                    if !span
                        .tiles()
                        .into_iter()
                        .all(|t| self.demo_water(t) == Some(t != from && t != to))
                    {
                        continue;
                    }
                    let general = TileCoord::new(from.x - dx * 20, from.y - dy * 20);
                    let goal = TileCoord::new(to.x + dx * 24, to.y + dy * 24);
                    let clear = |t| {
                        position(t).is_ok_and(|p| {
                            self.config.valid_ground_position(p)
                                && self.body_position_clear(UnitKind::BatteringRam, p)
                        })
                    };
                    if !clear(general) || !clear(goal) {
                        continue;
                    }
                    let mut soldiers: Vec<_> = (0..625)
                        .map(|n| {
                            let along = -26 + n / 25;
                            let across = n % 25 - 12;
                            TileCoord::new(
                                from.x + dx * along - dy * across,
                                from.y + dy * along + dx * across,
                            )
                        })
                        .filter(|&t| clear(t))
                        .collect();
                    let centre = TileCoord::new(from.x - dx * 13, from.y - dy * 13);
                    soldiers.sort_unstable_by_key(|t| {
                        ((t.x - centre.x).pow(2) + (t.y - centre.y).pow(2), t.y, t.x)
                    });
                    soldiers.truncate(100);
                    if soldiers.len() == 100 && !soldiers.contains(&general) {
                        return Some((span, general, goal, soldiers));
                    }
                }
                None
            })
            .ok_or(EconomyError::InvalidFootprint)?;
        let (span, general, goal, soldiers) = found;
        for id in self.units().map(|u| u.id).collect::<Vec<_>>() {
            if let Some(&index) = self.lookup.get(&id)
                && self.units[index].state.hp > 0
            {
                self.kill_unit(index);
            }
            self.remove_unit(id);
        }
        self.config.tick_hz = 5; // Four-times showcase clock, same movement rules.
        self.set_fog_disabled(true);
        self.insert_building_shape(
            PlayerId(0),
            BuildingKind::Bridge,
            span.footprint(),
            true,
            Some(span),
        )?;
        if let Terrain::Map { overlay, .. } = &mut self.terrain {
            overlay
                .add_bridge(span)
                .map_err(|_| EconomyError::Capacity)?;
        }
        for tile in span.tiles() {
            self.economy.blocked_tiles.remove(&tile);
        }
        self.collision_opened(span.footprint());
        for (n, tile) in soldiers.into_iter().enumerate() {
            let kind = match n {
                0..40 => UnitKind::Militia,
                40..70 => UnitKind::Archer,
                70..90 => UnitKind::Knight,
                _ => UnitKind::BatteringRam,
            };
            self.spawn_kind(PlayerId(0), kind, position(tile)?)?;
        }
        let g = self.spawn_kind(PlayerId(0), UnitKind::General, position(general)?)?;
        Ok((g, span, position(goal)?))
    }

    /// Stand a finished bridge on map water, for local real-map measurements.
    pub fn place_finished_bridge(&mut self, span: BridgeSpan) -> Result<(), EconomyError> {
        self.place_bridge_for(PlayerId(0), span)
    }

    /// Stand a finished bridge of `player` on map water (scenario setup).
    pub(super) fn place_bridge_for(
        &mut self,
        player: PlayerId,
        span: BridgeSpan,
    ) -> Result<(), EconomyError> {
        self.insert_building_shape(
            player,
            BuildingKind::Bridge,
            span.footprint(),
            true,
            Some(span),
        )?;
        if let Terrain::Map { overlay, .. } = &mut self.terrain {
            overlay
                .add_bridge(span)
                .map_err(|_| EconomyError::Capacity)?;
        }
        for tile in span.tiles() {
            self.economy.blocked_tiles.remove(&tile);
        }
        self.collision_opened(span.footprint());
        Ok(())
    }

    pub(super) fn demo_water(&self, tile: TileCoord) -> Option<bool> {
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
            Some(true)
        } else if sample.water == WaterKind::None && sample.passable {
            Some(false)
        } else {
            None
        }
    }
}

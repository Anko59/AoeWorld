use super::{GameWorld, GameWorldError, Terrain, UnitKind};
use aoe_core::{EntityId, PlayerId, TileCoord, WorldPosition};
use aoe_map::{EnvironmentPageError, GroundMaterial, LandBucket, WaterKind};
use std::collections::BTreeMap;

mod lifecycle;
mod persistence;
use lifecycle::RelicTarget;
pub(crate) use lifecycle::Relics;

/// Adapt DE Arabia's balanced relics to geographic maps. Small games get
/// two relics per seat and one contested relic. More seats keep a local
/// relic each, with more neutral relics on larger maps.
fn distribution(side: i32) -> (usize, usize) {
    if side <= 144 {
        (2, 1)
    } else {
        (1, (side as u32 / 128).ilog2().clamp(1, 8) as usize + 2)
    }
}

fn distance(a: TileCoord, b: TileCoord) -> i32 {
    (a.x - b.x).abs().max((a.y - b.y).abs())
}

fn ring(center: TileCoord, radius: i32, point: i32) -> TileCoord {
    let wide = radius * 2;
    let offset = point.rem_euclid(wide) - radius;
    let (x, y) = match point.rem_euclid(wide * 4) / wide {
        0 => (offset, -radius),
        1 => (radius, offset),
        2 => (-offset, radius),
        _ => (-radius, -offset),
    };
    TileCoord::new(center.x + x, center.y + y)
}

impl GameWorld {
    /// Seed neutral relics after the villages establish their terrain
    /// reservations. A site needs a positive Monk navigation witness.
    pub fn place_relics(
        &mut self,
        scouts: &[EntityId],
        cancelled: &dyn Fn() -> bool,
    ) -> Result<(), GameWorldError> {
        if scouts.is_empty() || !self.relics.is_empty() {
            return Ok(());
        }
        let starts = scouts
            .iter()
            .map(|&id| self.unit(id).ok_or(GameWorldError::UnknownEntity))
            .collect::<Result<Vec<_>, _>>()?;
        let starts: Vec<_> = starts
            .iter()
            .map(|u| (u.player, u.position.tile_floor()))
            .collect();
        let side = self.config.width_tiles.min(self.config.height_tiles);
        let (local, neutral) = distribution(side);
        let radius = (side / 4).clamp(12, 32);
        let spacing = (radius / 2).max(6);
        let bucket = LandBucket::for_radius(UnitKind::Monk.collision_radius())
            .ok_or(GameWorldError::InvalidPosition)?;
        let mut sites = Vec::with_capacity(local * starts.len() + neutral);
        let mut try_site = |tile, owners: &[(PlayerId, TileCoord)]| {
            if cancelled() {
                return Err(GameWorldError::Environment(EnvironmentPageError::Cancelled));
            }
            if sites.iter().any(|&site| distance(tile, site) < spacing)
                || starts.iter().any(|&(_, start)| distance(tile, start) < 8)
                || !self.relic_site_open(tile, cancelled)?
            {
                return Ok(false);
            }
            for &(player, from) in owners {
                if !self
                    .land_reachable_bounded(player, bucket, from, tile)?
                    .is_reachable()
                {
                    return Ok(false);
                }
            }
            sites.push(tile);
            Ok(true)
        };

        // Give each seat the same first chance before placing its second relic.
        for round in 0..local {
            for &(player, start) in &starts {
                let owners = [(player, start)];
                let seed = self.start_roll(start, 0x5245_4c49_4300 + round as u64);
                'local: for r in [radius, radius - 6, radius + 6, 12, radius + 16] {
                    let offset = (seed % (8 * r) as u64) as i32;
                    for n in 0..24 {
                        let tile = ring(start, r, offset + n * 8 * r / 24);
                        if starts.iter().all(|&(_, other)| distance(tile, other) >= r)
                            && try_site(tile, &owners)?
                        {
                            break 'local;
                        }
                    }
                }
            }
        }

        // Contested sites sit between seats and must be reachable from both.
        // For a solo game, search an outer ring around its one village.
        'neutral: for n in 0..neutral {
            let a = n * starts.len() / neutral;
            let owners = if starts.len() == 1 {
                vec![starts[a]]
            } else {
                let other = starts
                    .iter()
                    .enumerate()
                    .filter(|&(i, _)| i != a)
                    .min_by_key(|&(_, &(_, at))| distance(starts[a].1, at))
                    .map(|(_, &start)| start)
                    .unwrap_or(starts[a]);
                vec![starts[a], other]
            };
            let from = owners[0].1;
            let to = owners.last().map_or(from, |owner| owner.1);
            let middle = TileCoord::new((from.x + to.x) / 2, (from.y + to.y) / 2);
            let seed = self.start_roll(middle, 0x4e45_5554_5200 + n as u64);
            // Geographic seats may be continents apart. Limit midpoint
            // probes to town-sized routes and use a wider local ring if
            // there is no positively certified common site.
            if distance(from, to) <= 128 {
                for r in [6, 12, 24, 36, 48] {
                    let offset = (seed % (8 * r) as u64) as i32;
                    for k in 0..16 {
                        let tile = ring(middle, r, offset + k * 8 * r / 16);
                        if try_site(tile, &owners)? {
                            continue 'neutral;
                        }
                    }
                }
            }
            for r in [radius + 16, radius + 32, radius - 4, 12] {
                let offset = (seed % (8 * r) as u64) as i32;
                for k in 0..24 {
                    let tile = ring(from, r, offset + k * 8 * r / 24);
                    if try_site(tile, &owners[..1])? {
                        continue 'neutral;
                    }
                }
            }
        }
        for site in sites {
            let position = WorldPosition::from_tile_center(site)?;
            let id = self.allocate_entity_id()?;
            self.relics.ground.insert(id, position);
        }
        Ok(())
    }

    fn relic_site_open(
        &self,
        tile: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<bool, GameWorldError> {
        if !self.tile_free(tile) {
            return Ok(false);
        }
        let Terrain::Map { generator, .. } = &self.terrain else {
            return Ok(true);
        };
        Ok(generator
            .tile_at_with_cancel(tile, cancelled)?
            .is_some_and(|t| {
                t.water == WaterKind::None && t.material != GroundMaterial::ForestFloor
            }))
    }

    pub fn ground_relics(&self) -> impl Iterator<Item = (EntityId, WorldPosition)> + '_ {
        self.relics.ground.iter().map(|(&id, &at)| (id, at))
    }
}

#[cfg(test)]
#[path = "tests/lifecycle.rs"]
mod lifecycle_tests;
#[cfg(test)]
mod tests;

//! Fixed-radius, memoized site ranking; never scans a map-sized region.
use super::{EconomyError, VILLAGE_RULES, placement::distance_squared};
use crate::{GameWorld, GameWorldError, Terrain};
use aoe_core::{TileCoord, TileRect};
use aoe_map::{GroundMaterial, ResourceKind, ResourceNode, Tile, WaterKind};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

const SAMPLE_BUDGET: usize = 12_000;
const SITE_BUDGET: usize = 384;
/// The immediate clearing: squared distance of the sites scored first, and the
/// reach of the walkable region that suffices to survey it.
const CORE_D2: u64 = 36;
const CORE_REGION: i32 = 12;
type Samples = BTreeMap<TileCoord, Option<(Tile, Option<ResourceNode>)>>;

impl GameWorld {
    pub(super) fn rank_start_sites(
        &self,
        candidates: &mut Vec<TileCoord>,
        start: TileCoord,
    ) -> Result<(), EconomyError> {
        let Terrain::Map { generator, .. } = &self.terrain else {
            return Ok(());
        };
        let mut samples = Samples::new();
        // The immediate clearing first. An inland site there is the village the
        // ideal point asked for, and surveying only its surroundings composes
        // a few chunks instead of the sixteen of the whole search radius, which
        // dominated placing a hundred seats. Coast and cliff fall through.
        let near = self.start_region(start, CORE_REGION);
        let mut core: Vec<TileCoord> = candidates
            .iter()
            .copied()
            .filter(|tile| distance_squared(*tile, start) <= CORE_D2 && near.contains(tile))
            .collect();
        let scores = self.score_sites(generator, &core, &mut samples)?;
        if scores.values().any(|(shore, _)| !shore) {
            sort_by_score(&mut core, &scores);
            *candidates = core;
            return Ok(());
        }
        // Local tree/shore ranking must not jump across water or cliffs into
        // a disconnected bowl after the global survey selected a valley.
        let reachable = self.start_region(start, VILLAGE_RULES.start_search_radius_tiles + 4);
        candidates.retain(|tile| reachable.contains(tile));
        // Fine search in the immediate clearing, then a four-tile grid through
        // the full 32-tile radius. Unscored tiles remain a deterministic fallback.
        let sites: Vec<TileCoord> = candidates
            .iter()
            .copied()
            .filter(|tile| {
                distance_squared(*tile, start) <= CORE_D2
                    || ((tile.x - start.x) % 4 == 0 && (tile.y - start.y) % 4 == 0)
            })
            .take(SITE_BUDGET)
            .collect();
        let scores = self.score_sites(generator, &sites, &mut samples)?;
        sort_by_score(candidates, &scores);
        Ok(())
    }

    /// Shore and wood in view of each site whose footprint is free.
    fn score_sites(
        &self,
        generator: &aoe_map::MapChunkGenerator,
        sites: &[TileCoord],
        samples: &mut Samples,
    ) -> Result<BTreeMap<TileCoord, (bool, bool)>, EconomyError> {
        let side = VILLAGE_RULES.town_center_side;
        let mut scores = BTreeMap::new();
        for origin in sites.iter().copied() {
            let rect = TileRect::new(
                origin,
                TileCoord::new(origin.x.saturating_add(side), origin.y.saturating_add(side)),
            );
            match self.validate_footprint(rect) {
                Ok(()) => {}
                Err(EconomyError::InvalidFootprint) => continue,
                Err(error) => return Err(error),
            }
            let center = TileCoord::new(origin.x + side / 2, origin.y + side / 2);
            let mut shore = false;
            let mut wood = 0;
            for dy in -12_i32..=12 {
                for dx in -12_i32..=12 {
                    if dx * dx + dy * dy > 144 {
                        continue;
                    }
                    let tile = TileCoord::new(center.x + dx, center.y + dy);
                    let full = samples.len() == SAMPLE_BUDGET;
                    if let std::collections::btree_map::Entry::Vacant(entry) = samples.entry(tile) {
                        if full {
                            break;
                        }
                        entry.insert(
                            generator
                                .tile_and_node_with_cancel(tile, &|| false)
                                .map_err(GameWorldError::Environment)?,
                        );
                    }
                    if let Some(Some((terrain, node))) = samples.get(&tile) {
                        if dx.abs() <= 4 && dy.abs() <= 4 {
                            shore |= terrain.water != WaterKind::None
                                || matches!(
                                    terrain.material,
                                    GroundMaterial::Sand | GroundMaterial::Shore
                                );
                        }
                        wood += usize::from(node.is_some_and(|n| n.kind == ResourceKind::Wood));
                    }
                }
            }
            scores.insert(origin, (shore, wood == 0));
            // The ranking key prefers inland sites with wood in view, then the
            // earliest candidate. Candidates are scored in that order, so the
            // first such site is the final winner: scoring the rest of the
            // 384 (about 12,000 point samples) cannot change the choice.
            if !shore && wood > 0 {
                break;
            }
            if samples.len() == SAMPLE_BUDGET {
                break;
            }
        }
        Ok(scores)
    }
}

/// Inland sites with wood in view first, then the nearest candidates.
fn sort_by_score(candidates: &mut [TileCoord], scores: &BTreeMap<TileCoord, (bool, bool)>) {
    let original: BTreeMap<_, _> = candidates
        .iter()
        .enumerate()
        .map(|(i, t)| (*t, i))
        .collect();
    candidates.sort_by_key(|tile| {
        let (shore, no_wood) = scores.get(tile).copied().unwrap_or((true, true));
        (!scores.contains_key(tile), shore, no_wood, original[tile])
    });
}

impl GameWorld {
    fn start_region(&self, start: TileCoord, radius: i32) -> BTreeSet<TileCoord> {
        let mut seeds = (-3_i32..=3)
            .flat_map(|dy| {
                (-3_i32..=3).map(move |dx| {
                    TileCoord::new(start.x.saturating_add(dx), start.y.saturating_add(dy))
                })
            })
            .collect::<Vec<_>>();
        seeds.sort_by_key(|tile| (distance_squared(*tile, start), tile.y, tile.x));
        let Some(root) = seeds.into_iter().find(|tile| self.tile_passable(*tile)) else {
            return BTreeSet::new();
        };
        let mut reached = BTreeSet::from([root]);
        let mut queue = VecDeque::from([root]);
        while let Some(tile) = queue.pop_front() {
            for (dx, dy) in [(0, -1), (-1, 0), (1, 0), (0, 1)] {
                let next = TileCoord::new(tile.x.saturating_add(dx), tile.y.saturating_add(dy));
                if next.x.abs_diff(start.x) > radius as u32
                    || next.y.abs_diff(start.y) > radius as u32
                    || reached.contains(&next)
                    || !self.tile_passable(next)
                    || !self.terrain.crossable(tile, next, self.config)
                {
                    continue;
                }
                reached.insert(next);
                queue.push_back(next);
            }
        }
        reached
    }
}

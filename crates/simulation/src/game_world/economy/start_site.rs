//! Fixed-radius, memoized site ranking; never scans a map-sized region.
use super::{EconomyError, VILLAGE_RULES, placement::distance_squared};
use crate::{GameWorld, GameWorldError, Terrain};
use aoe_core::{TileCoord, TileRect};
use aoe_map::{GroundMaterial, ResourceKind, ResourceNode, Tile, WaterKind};
use std::collections::BTreeMap;

const SAMPLE_BUDGET: usize = 12_000;
const SITE_BUDGET: usize = 384;
type Samples = BTreeMap<TileCoord, Option<(Tile, Option<ResourceNode>)>>;

impl GameWorld {
    pub(super) fn rank_start_sites(
        &self,
        candidates: &mut [TileCoord],
        start: TileCoord,
    ) -> Result<(), EconomyError> {
        let Terrain::Map { generator, .. } = &self.terrain else {
            return Ok(());
        };
        let side = VILLAGE_RULES.town_center_side;
        let mut samples = Samples::new();
        let mut scores = BTreeMap::new();
        // Fine search in the immediate clearing, then a four-tile grid through
        // the full 32-tile radius. Unscored tiles remain a deterministic fallback.
        for origin in candidates
            .iter()
            .copied()
            .filter(|tile| {
                distance_squared(*tile, start) <= 36
                    || ((tile.x - start.x) % 4 == 0 && (tile.y - start.y) % 4 == 0)
            })
            .take(SITE_BUDGET)
        {
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
            if samples.len() == SAMPLE_BUDGET {
                break;
            }
        }
        let original: BTreeMap<_, _> = candidates
            .iter()
            .enumerate()
            .map(|(i, t)| (*t, i))
            .collect();
        candidates.sort_by_key(|tile| {
            let (shore, no_wood) = scores.get(tile).copied().unwrap_or((true, true));
            (!scores.contains_key(tile), shore, no_wood, original[tile])
        });
        Ok(())
    }
}

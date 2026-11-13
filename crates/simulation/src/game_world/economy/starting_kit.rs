//! Bounded local resource placement; paths are certified before adding blockers.
use super::EconomyError;
use super::placement::{distance_squared, perimeter};
use crate::{GameWorld, GameWorldError, Terrain};
use aoe_core::{TileCoord, TileRect};
use aoe_map::{ResourceKind, ResourceNode};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

const REACH: i32 = 15;
const ANCHOR_BUDGET: usize = 128;
type Graph = BTreeMap<TileCoord, Vec<TileCoord>>;
const STEPS: [(i32, i32); 4] = [(0, -1), (-1, 0), (1, 0), (0, 1)];

impl GameWorld {
    pub fn starting_resources(&self) -> Vec<ResourceNode> {
        match &self.terrain {
            Terrain::Map { overlay, .. } => overlay.starting_nodes().collect(),
            _ => Vec::new(),
        }
    }

    pub(super) fn initialize_starting_kit(
        &mut self,
        footprint: TileRect,
        root: TileCoord,
    ) -> Result<(), EconomyError> {
        let Terrain::Map { generator, overlay } = &self.terrain else {
            return Ok(());
        };
        let center = TileCoord::new(footprint.min.x + 2, footprint.min.y + 2);
        // A restored game already contains this village's original kit,
        // including exhausted nodes. Another village's kit does not count: a
        // match seats many villages in one world.
        if overlay
            .starting_nodes()
            .any(|node| distance_squared(node.tile, center) <= (REACH as u64).pow(2))
        {
            return Ok(());
        }
        let exits: Vec<_> = perimeter(footprint)
            .into_iter()
            .filter(|t| self.tile_passable(*t))
            .collect();
        let graph = self.starting_graph(center, root);
        let initial = reachable(&graph, root, &BTreeSet::new());
        let exits: Vec<_> = exits
            .into_iter()
            .filter(|tile| initial.contains(tile))
            .collect();
        let mut natural = Vec::new();
        for tile in initial.iter().flat_map(|tile| adjacent(*tile)) {
            if distance_squared(tile, center) <= (REACH as u64).pow(2)
                && let Some(node) = generator
                    .object_at_with_cancel(tile, &|| false)
                    .map_err(GameWorldError::Environment)?
                && overlay.blocks_node(node)
            {
                natural.push(node);
            }
        }
        natural.sort_by_key(|n| n.id);
        natural.dedup_by_key(|n| n.id);
        let natural_tiles: BTreeSet<TileCoord> = natural.iter().map(|n| n.tile).collect();
        let mut planned = Vec::new();
        let mut blocked = BTreeSet::new();
        let mut incomplete = false;
        let food = 7 + (self.start_roll(center, 0) % 2) as usize;
        let wood = 12 + (self.start_roll(center, 1) % 2) as usize;
        for (slot, (kind, target, minimum, maximum)) in [
            (ResourceKind::Food, food, 5_u64, 10_u64),
            (ResourceKind::Gold, 7, 7, 14),
            (ResourceKind::Stone, 5, 7, 14),
            (ResourceKind::Wood, wood - 3, 7, 15),
            (ResourceKind::Wood, wood - 2, 3, 6),
            (ResourceKind::Wood, wood - 1, 3, 6),
            (ResourceKind::Wood, wood, 3, 6),
        ]
        .into_iter()
        .enumerate()
        {
            // Stragglers top up the combined wood budget; other groups count
            // only natural nodes in their own band, not a distant berry patch.
            let counted_band = if kind == ResourceKind::Wood && slot >= 4 {
                9..=225
            } else {
                minimum.pow(2)..=maximum.pow(2)
            };
            let count = target.saturating_sub(
                natural
                    .iter()
                    .chain(planned.iter())
                    .filter(|n| {
                        n.kind == kind && counted_band.contains(&distance_squared(n.tile, center))
                    })
                    .count(),
            );
            if count == 0 {
                continue;
            }
            let salt = 10 + slot as u64;
            // Straggler wood slots fall back to the wider woodline band when
            // the close band has no access-safe tiles left.
            let bands: &[(u64, u64)] = if kind == ResourceKind::Wood && slot >= 4 {
                &[(minimum, maximum), (7, 15)]
            } else {
                &[(minimum, maximum)]
            };
            let mut remaining = count;
            for &(minimum, maximum) in bands {
                if remaining == 0 {
                    break;
                }
                let mut anchors: Vec<_> = initial
                    .iter()
                    .copied()
                    .filter(|tile| {
                        let d = distance_squared(*tile, center);
                        (minimum.pow(2)..=maximum.pow(2)).contains(&d)
                    })
                    .collect();
                // Every player gets a different direction and distance, rather than
                // rotating the same stamped kit. The bounded fallback stays local.
                let anchor_salt = self.start_roll(center, salt);
                let Some(preferred) = anchors
                    .iter()
                    .copied()
                    .min_by_key(|tile| self.start_roll(*tile, anchor_salt))
                else {
                    continue;
                };
                anchors.sort_by_key(|tile| {
                    (
                        distance_squared(*tile, preferred),
                        self.start_roll(*tile, salt),
                    )
                });
                while remaining > 0 {
                    // Free ground the kit has not yet cut off: a cluster may not
                    // seal any of it into a pocket, however small.
                    let open_before = reachable(&graph, root, &blocked);
                    let mut selected = None;
                    for cluster_size in (1..=remaining).rev() {
                        // A one-tile cluster must lean on something solid, so only
                        // anchors touching a blocker are tried for it.
                        let candidates = anchors.iter().copied().filter(|tile| {
                            cluster_size > 1
                                || (-1..=1).any(|dy| {
                                    (-1..=1).any(|dx| {
                                        let next = TileCoord::new(tile.x + dx, tile.y + dy);
                                        next != *tile
                                            && (blocked.contains(&next)
                                                || natural_tiles.contains(&next)
                                                || !self.tile_passable(next)
                                                || footprint.contains(next))
                                    })
                                })
                        });
                        for anchor in candidates.take(ANCHOR_BUDGET) {
                            for attempt in 0..2 {
                                let tiles =
                                    self.start_cluster(anchor, cluster_size, salt + attempt * 100);
                                if tiles.iter().any(|tile| {
                                    !initial.contains(tile)
                                        || blocked.contains(tile)
                                        || !self.tile_free(*tile)
                                        || !(minimum.pow(2)..=maximum.pow(2))
                                            .contains(&distance_squared(*tile, center))
                                }) {
                                    continue;
                                }
                                let mut trial = blocked.clone();
                                trial.extend(&tiles);
                                let reached = reachable(&graph, root, &trial);
                                if !exits.iter().all(|tile| reached.contains(tile))
                                    || !natural
                                        .iter()
                                        .chain(planned.iter())
                                        .map(|n: &ResourceNode| n.tile)
                                        .chain(tiles.iter().copied())
                                        .all(|tile| accessible(tile, &reached))
                                {
                                    continue;
                                }
                                // Enclosure: no free tile newly sealed off, and no
                                // lone blocker in open ground (a boxed-in villager
                                // is the playtests' most reported bug).
                                if open_before
                                    .iter()
                                    .any(|t| !trial.contains(t) && !reached.contains(t))
                                    || tiles.iter().any(|tile| {
                                        !(-1..=1).any(|dy| {
                                            (-1..=1).any(|dx| {
                                                let next = TileCoord::new(tile.x + dx, tile.y + dy);
                                                next != *tile
                                                    && (tiles.contains(&next)
                                                        || blocked.contains(&next)
                                                        || natural_tiles.contains(&next)
                                                        || (!initial.contains(&next)
                                                            && (!self.tile_passable(next)
                                                                || footprint.contains(next))))
                                            })
                                        })
                                    })
                                {
                                    continue;
                                }
                                selected = Some((tiles, trial));
                                break;
                            }
                            if selected.is_some() {
                                break;
                            }
                        }
                        if selected.is_some() {
                            break;
                        }
                    }
                    // Tiny valid valleys still start when a full cluster cannot fit.
                    let Some((tiles, trial)) = selected else {
                        break;
                    };
                    remaining -= tiles.len();
                    blocked = trial;
                    for tile in tiles {
                        let node = if kind == ResourceKind::Wood {
                            generator
                                .starting_tree_at(tile)
                                .map_err(GameWorldError::Environment)?
                        } else {
                            ResourceNode::starting(tile, kind)
                        }
                        .ok_or(EconomyError::InvalidFootprint)?;
                        planned.push(node);
                    }
                }
            }
            incomplete |= remaining > 0;
        }
        let Terrain::Map { generator, overlay } = &mut self.terrain else {
            unreachable!()
        };
        let wanted = planned.len();
        let mut inserted = 0_usize;
        for node in planned {
            if overlay.insert_starting_resource(generator, node).is_err() {
                break;
            }
            inserted += 1;
        }
        // Nodes inserted so far are part of the world even when a later one was
        // refused: the caches and collision state must see them either way.
        if inserted > 0 {
            if let Ok(cache) = self.resource_chunks.get_mut() {
                cache.clear();
            }
            self.resource_collision_changed();
        }
        if incomplete || inserted < wanted {
            self.partial_kits += 1;
            eprintln!(
                "starting kit near {center:?} is partial: {inserted} of {wanted} planned nodes; constrained={incomplete}"
            );
            return Err(EconomyError::InvalidFootprint);
        }
        Ok(())
    }

    /// How many villages of this world started with a kit that could not be
    /// placed in full (genuinely impossible terrain; a healthy match has 0).
    pub fn partial_starting_kits(&self) -> u64 {
        self.partial_kits
    }

    /// Coordinate hashing is used only at village creation, never per tick.
    pub(crate) fn start_roll(&self, tile: TileCoord, domain: u64) -> u64 {
        let mut input = [0_u8; 24];
        input[..8].copy_from_slice(&self.config.seed.0.to_le_bytes());
        input[8..12].copy_from_slice(&tile.x.to_le_bytes());
        input[12..16].copy_from_slice(&tile.y.to_le_bytes());
        input[16..].copy_from_slice(&domain.to_le_bytes());
        let mut bytes = [0_u8; 8];
        bytes.copy_from_slice(&blake3::hash(&input).as_bytes()[..8]);
        u64::from_le_bytes(bytes)
    }

    fn start_cluster(&self, anchor: TileCoord, count: usize, salt: u64) -> Vec<TileCoord> {
        let mut candidates: Vec<_> = (-4..=4)
            .flat_map(|y| (-4..=4).map(move |x| TileCoord::new(anchor.x + x, anchor.y + y)))
            .collect();
        candidates.sort_by_key(|tile| {
            distance_squared(*tile, anchor) * 8 + self.start_roll(*tile, salt) % 64
        });
        candidates.truncate(count);
        candidates
    }

    fn starting_graph(&self, center: TileCoord, root: TileCoord) -> Graph {
        let mut graph = Graph::new();
        let mut queue = VecDeque::from([root]);
        let mut seen = BTreeSet::from([root]);
        while let Some(tile) = queue.pop_front() {
            let edges: Vec<_> = adjacent(tile)
                .filter(|next| {
                    distance_squared(*next, center) <= (REACH as u64).pow(2)
                        && self.tile_passable(*next)
                        && self.terrain.crossable(tile, *next, self.config)
                })
                .collect();
            for next in &edges {
                if seen.insert(*next) {
                    queue.push_back(*next);
                }
            }
            graph.insert(tile, edges);
        }
        graph
    }
}

fn adjacent(tile: TileCoord) -> impl Iterator<Item = TileCoord> {
    STEPS
        .into_iter()
        .map(move |(x, y)| TileCoord::new(tile.x + x, tile.y + y))
}
fn accessible(tile: TileCoord, reached: &BTreeSet<TileCoord>) -> bool {
    adjacent(tile).any(|t| reached.contains(&t))
}
fn reachable(graph: &Graph, root: TileCoord, blocked: &BTreeSet<TileCoord>) -> BTreeSet<TileCoord> {
    if blocked.contains(&root) {
        return BTreeSet::new();
    }
    let mut reached = BTreeSet::from([root]);
    let mut queue = VecDeque::from([root]);
    while let Some(tile) = queue.pop_front() {
        for next in graph.get(&tile).into_iter().flatten() {
            if !blocked.contains(next) && reached.insert(*next) {
                queue.push_back(*next);
            }
        }
    }
    reached
}

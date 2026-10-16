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
        // A restored game already contains its original kit, including exhausted nodes.
        if overlay.starting_nodes().next().is_some() {
            return Ok(());
        }
        let center = TileCoord::new(footprint.min.x + 2, footprint.min.y + 2);
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
        let mut planned = Vec::new();
        let mut blocked = BTreeSet::new();
        for (kind, count, radius, dx, dy) in [
            (ResourceKind::Food, 7_usize, 6, -5, -3),
            (ResourceKind::Gold, 7, 9, 7, -6),
            (ResourceKind::Stone, 5, 10, 8, 6),
            (ResourceKind::Wood, 12, 8, -7, 4),
        ] {
            let count = count.saturating_sub(natural.iter().filter(|n| n.kind == kind).count());
            if count == 0 {
                continue;
            }
            let preferred = TileCoord::new(center.x + dx, center.y + dy);
            let mut anchors: Vec<_> = initial.iter().copied().collect();
            anchors.sort_by_key(|tile| {
                (
                    distance_squared(*tile, preferred),
                    distance_squared(*tile, center).abs_diff((radius as u64).pow(2)),
                    tile.y,
                    tile.x,
                )
            });
            let mut selected = None;
            for cluster_size in [count, 1] {
                for anchor in anchors.iter().copied().take(ANCHOR_BUDGET) {
                    for rotate in [false, true] {
                        let tiles: Vec<_> = (0..cluster_size)
                            .map(|i| {
                                let (x, y) =
                                    ((i / 2) as i32 - (cluster_size / 4) as i32, (i % 2) as i32);
                                TileCoord::new(
                                    anchor.x + if rotate { y } else { x },
                                    anchor.y + if rotate { x } else { y },
                                )
                            })
                            .collect();
                        if tiles.iter().any(|tile| {
                            !initial.contains(tile)
                                || blocked.contains(tile)
                                || !self.tile_free(*tile)
                                || distance_squared(*tile, center)
                                    < if cluster_size == 1 { 9 } else { 25 }
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
                continue;
            };
            blocked = trial;
            for tile in tiles {
                planned.push(
                    ResourceNode::starting(tile, kind).ok_or(EconomyError::InvalidFootprint)?,
                );
            }
        }
        let Terrain::Map { generator, overlay } = &mut self.terrain else {
            unreachable!()
        };
        for node in planned {
            overlay
                .insert_starting_resource(generator, node)
                .map_err(|_| EconomyError::InvalidFootprint)?;
        }
        self.resource_collision_changed();
        Ok(())
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

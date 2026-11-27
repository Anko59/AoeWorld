//! Owned road planning state, resumed in bounded polls.
use super::super::placement::{distance_squared, perimeter};
use super::plan::{Site, TIER_ROAD};
use super::road_search::Search;
use super::roads::{Out, STUB, TIER_GATE_ROAD, View, center_of, footprint_of};
use super::*;
#[derive(Debug)]
pub(super) struct Network {
    pub(super) player: PlayerId,
    pub(super) avoid: BTreeSet<TileCoord>,
    pub(super) gate_tiles: BTreeSet<TileCoord>,
    /// All castle entrances until the first route selects one. Thereafter
    /// only tiles physically joined to that entrance.
    pub(super) goals: BTreeSet<TileCoord>,
    pub(super) rooted: bool,
    pub(super) out: Vec<Site>,
    pub(super) roads: BTreeSet<TileCoord>,
    /// Own bridges already laid (even as foundations), by bank: the route
    /// uses them as they are instead of planning a parallel span.
    pub(super) fixed: BTreeMap<TileCoord, (TileCoord, u32)>,
    /// Spans already included in this plan, keyed by canonical bank order.
    pub(super) planned_spans: BTreeSet<(TileCoord, TileCoord)>,
}

#[derive(Debug)]
struct Endpoint {
    node: TileRect,
    out: Out,
    tier: u8,
    center: TileCoord,
    nearest: u64,
}

pub(super) struct Planner {
    net: Network,
    remaining: Vec<Endpoint>,
    tail: Vec<(TileCoord, u8, bool)>,
    tail_at: usize,
    search: Option<Search>,
}
impl std::fmt::Debug for Planner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RoadPlanner")
    }
}
fn edge(r: TileRect, out: Out) -> Vec<TileCoord> {
    let c = center_of(r);
    perimeter(r)
        .into_iter()
        .filter(|t| {
            let side = (r.min.x..r.max.x).contains(&t.x) || (r.min.y..r.max.y).contains(&t.y);
            side && out.is_none_or(|o| (t.x - c.x) * o.0 + (t.y - c.y) * o.1 < 0)
        })
        .collect()
}
impl Planner {
    pub(super) fn new(
        w: &GameWorld,
        center: TileCoord,
        player: PlayerId,
        planned: &[Site],
    ) -> Option<Self> {
        let castle = w
            .player_buildings_of(player, BuildingKind::Castle)
            .find(|b| b.footprint.contains(center))?;
        // Work points anywhere in the domain, not just inside the wall square.
        let reach = VASSAL_RADIUS;
        let in_domain = |r: TileRect| {
            let c = center_of(r);
            (c.x - center.x).abs() <= reach && (c.y - center.y).abs() <= reach
        };
        // Include every gate and each distinct operating or planned work
        // and production site in the domain.
        let mut production: BTreeSet<_> = planned
            .iter()
            .filter(|s| {
                matches!(
                    s.kind,
                    BuildingKind::Barracks | BuildingKind::ArcheryRange | BuildingKind::Stable
                )
            })
            .map(footprint_of)
            .map(|r| (r.min, r.max))
            .collect();
        let mut stubs: Vec<(TileCoord, (i32, i32))> = Vec::new();
        let mut nodes: Vec<(TileRect, Out, u8)> = Vec::new();
        for gate in planned.iter().filter(|s| s.kind.is_gate()) {
            let rect = footprint_of(gate);
            let c = center_of(rect);
            // The wall's outward side is away from the castle along the gate's axis.
            let out = if gate.vertical {
                (if c.x < center.x { -1 } else { 1 }, 0)
            } else {
                (0, if c.y < center.y { -1 } else { 1 })
            };
            nodes.push((rect, Some(out), TIER_GATE_ROAD));
            stubs.push((c, out));
        }
        let mut standing: Vec<_> = [
            BuildingKind::TownCenter,
            BuildingKind::Mill,
            BuildingKind::LumberCamp,
            BuildingKind::MiningCamp,
            BuildingKind::Barracks,
            BuildingKind::ArcheryRange,
            BuildingKind::Stable,
            BuildingKind::SiegeWorkshop,
            BuildingKind::Monastery,
            BuildingKind::Dock,
        ]
        .into_iter()
        .flat_map(|k| w.player_buildings_of(player, k))
        .filter(|b| b.hp > 0 && b.is_complete() && in_domain(b.footprint))
        .map(|b| (b.id, b.footprint))
        .collect();
        standing
            .sort_unstable_by_key(|(id, rect)| (distance_squared(center, center_of(*rect)), *id));
        nodes.extend(
            production
                .iter()
                .map(|&(min, max)| (TileRect::new(min, max), None, TIER_ROAD)),
        );
        for (_, r) in standing {
            if production.insert((r.min, r.max)) {
                nodes.push((r, None, TIER_ROAD));
            }
        }
        // Footprints of every other planned building stay clear of roads.
        let avoid: BTreeSet<TileCoord> = planned
            .iter()
            .filter(|s| {
                !matches!(s.kind, BuildingKind::Road | BuildingKind::Bridge) && !s.kind.is_gate()
            })
            .flat_map(|s| {
                let r = footprint_of(s);
                (r.min.y..r.max.y)
                    .flat_map(move |y| (r.min.x..r.max.x).map(move |x| TileCoord::new(x, y)))
            })
            .collect();
        // Routes pass through planned gates (the Vassal builds them), but never
        // lay a road on one.
        let gate_tiles: BTreeSet<TileCoord> = planned
            .iter()
            .filter(|s| s.kind.is_gate())
            .flat_map(|s| {
                let r = footprint_of(s);
                (r.min.y..r.max.y)
                    .flat_map(move |y| (r.min.x..r.max.x).map(move |x| TileCoord::new(x, y)))
            })
            .collect();
        let net = Network {
            planned_spans: BTreeSet::new(),
            player,
            avoid,
            gate_tiles,
            goals: edge(castle.footprint, None).into_iter().collect(),
            rooted: false,
            out: Vec::new(),
            roads: BTreeSet::new(),
            fixed: w
                .player_buildings_of(player, BuildingKind::Bridge)
                .filter(|b| b.hp > 0)
                .filter_map(|b| b.bridge_span)
                .flat_map(|span| {
                    let len = span.tiles().len() as u32;
                    [(span.from, (span.to, len)), (span.to, (span.from, len))]
                })
                .collect(),
        };
        let unfinished: Vec<_> = w
            .player_buildings_of(player, BuildingKind::Road)
            .filter(|b| b.hp > 0 && !b.is_complete() && in_domain(b.footprint))
            .map(|b| (b.id, b.footprint.min))
            .collect::<BTreeMap<_, _>>()
            .into_values()
            .collect();

        let mut tail = Vec::new();
        for (c, (ox, oy)) in stubs {
            for n in 1..1 + STUB {
                tail.push((
                    TileCoord::new(c.x + ox * n, c.y + oy * n),
                    TIER_GATE_ROAD,
                    false,
                ));
            }
        }
        tail.extend(unfinished.into_iter().map(|tile| (tile, TIER_ROAD, true)));
        Some(Self {
            net,
            remaining: nodes
                .into_iter()
                .map(|(node, out, tier)| {
                    let node_center = center_of(node);
                    Endpoint {
                        node,
                        out,
                        tier,
                        center: node_center,
                        nearest: distance_squared(center, node_center),
                    }
                })
                .collect(),
            tail,
            tail_at: 0,
            search: None,
        })
    }
    pub(super) fn poll(&mut self, w: &GameWorld) -> Option<Vec<Site>> {
        let mut view = View::new(w, &mut self.net);
        // A per-domain AI work quantum, not a match-wide admission/refusal cap.
        // Pops, traceback and output each consume one operation.
        for _ in 0..128 {
            #[cfg(test)]
            super::super::bridge::work::operation();
            if let Some(search) = &mut self.search {
                if let Some(joined) = search.step(&mut view) {
                    // Each join contributes one new centre to the ranking.
                    // Keep its distance instead of re-reading the growing
                    // list of connected buildings for every remaining endpoint.
                    if joined {
                        let joined_center = center_of(search.node);
                        for endpoint in &mut self.remaining {
                            endpoint.nearest = endpoint
                                .nearest
                                .min(distance_squared(endpoint.center, joined_center));
                        }
                    }
                    self.search = None;
                }
            } else if let Some((index, _)) = self
                .remaining
                .iter()
                .enumerate()
                .min_by_key(|(_, e)| (e.tier, e.nearest, e.center.y, e.center.x))
            {
                let Endpoint {
                    node, out, tier, ..
                } = self.remaining.swap_remove(index);
                let edge = edge(node, out);
                let starts = edge
                    .iter()
                    .copied()
                    .filter(|t| !view.avoid.contains(t))
                    .collect::<Vec<_>>();
                self.search = Some(Search::new(&view, &starts, node, tier));
            } else if let Some(&(tile, tier, unfinished)) = self.tail.get(self.tail_at) {
                self.tail_at += 1;
                if unfinished {
                    if view.roads.insert(tile) {
                        view.out.push(Site::new(BuildingKind::Road, tile, tier));
                    }
                } else {
                    view.lay(tile, tier);
                }
            } else {
                return Some(std::mem::take(&mut view.out));
            }
        }
        None
    }
    pub(super) fn hash(&self, h: &mut blake3::Hasher) {
        // All collections below have canonical ordering. Search hashes its
        // mutation trace instead of iterating its lookup-only hash table.
        h.update(
            format!(
                "{:?}",
                (&self.net, &self.remaining, &self.tail, self.tail_at)
            )
            .as_bytes(),
        );
        if let Some(search) = &self.search {
            h.update(&[1]);
            search.hash(h);
        } else {
            h.update(&[0]);
        }
    }
}

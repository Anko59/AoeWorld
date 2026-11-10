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
    /// Tiles a route may end at: the joined nodes' edges and every road so far.
    pub(super) goals: BTreeSet<TileCoord>,
    pub(super) out: Vec<Site>,
    pub(super) roads: BTreeSet<TileCoord>,
    /// Own bridges already laid (even as foundations), by bank: the route
    /// uses them as they are instead of planning a parallel span.
    pub(super) fixed: BTreeMap<TileCoord, (TileCoord, u32)>,
    /// Banks of spans this plan already lays.
    pub(super) planned_spans: Vec<TileCoord>,
}

pub(super) struct Planner {
    net: Network,
    remaining: Vec<(TileRect, Out, u8)>,
    connected: Vec<TileCoord>,
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
        .filter(|t| out.is_none_or(|o| (t.x - c.x) * o.0 + (t.y - c.y) * o.1 <= 0))
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
        // Nodes: work points that already stand, then the planned gates.
        let mut nodes: Vec<(TileRect, Out)> = [
            BuildingKind::TownCenter,
            BuildingKind::Mill,
            BuildingKind::LumberCamp,
            BuildingKind::MiningCamp,
        ]
        .into_iter()
        .flat_map(|k| w.player_buildings_of(player, k))
        .filter(|b| b.hp > 0 && b.is_complete() && in_domain(b.footprint))
        .map(|b| (b.id, b.footprint))
        .collect::<BTreeMap<_, _>>()
        .into_values()
        .map(|r| (r, None))
        .collect();
        let local = nodes.len();
        let mut stubs: Vec<(TileCoord, (i32, i32))> = Vec::new();
        for gate in planned.iter().filter(|s| s.kind.is_gate()) {
            let rect = footprint_of(gate);
            let c = center_of(rect);
            // The wall's outward side is away from the castle along the gate's axis.
            let out = if gate.vertical {
                (if c.x < center.x { -1 } else { 1 }, 0)
            } else {
                (0, if c.y < center.y { -1 } else { 1 })
            };
            nodes.push((rect, Some(out)));
            stubs.push((c, out));
        }
        super::roads::trim_nodes(&mut nodes);
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
            planned_spans: Vec::new(),
            player,
            avoid,
            gate_tiles,
            goals: perimeter(castle.footprint).into_iter().collect(),
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
                .enumerate()
                .map(|(i, (r, o))| (r, o, if i < local { TIER_ROAD } else { TIER_GATE_ROAD }))
                .collect(),
            connected: vec![center_of(castle.footprint)],
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
            super::super::bridge_work::operation();
            if let Some(search) = &mut self.search {
                if let Some(joined) = search.step(&mut view) {
                    if joined {
                        self.connected.push(center_of(search.node));
                        view.goals.extend(search.edge.iter().copied());
                    }
                    self.search = None;
                }
            } else if !self.remaining.is_empty() {
                let nearest = |c: TileCoord| {
                    self.connected
                        .iter()
                        .map(|&o| distance_squared(c, o))
                        .min()
                        .unwrap_or(0)
                };
                let (i, _) = self
                    .remaining
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, (r, _, tier))| {
                        let c = center_of(*r);
                        (*tier, nearest(c), c.y, c.x)
                    })
                    .unwrap_or((0, &self.remaining[0]));
                let (node, out, tier) = self.remaining.swap_remove(i);
                let edge = edge(node, out);
                let starts = edge
                    .iter()
                    .copied()
                    .filter(|t| !view.avoid.contains(t))
                    .collect::<Vec<_>>();
                self.search = Some(Search::new(&view, &starts, node, edge, tier));
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
                (
                    &self.net,
                    &self.remaining,
                    &self.connected,
                    &self.tail,
                    self.tail_at
                )
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

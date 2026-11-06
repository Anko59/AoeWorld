//! The domain's road network: a tree from the castle to every Town Center,
//! mill, camp and gate, laid as one-tile roads on free open ground. Where a
//! river lies between two banks the route spans it with a bridge (straight,
//! at most `MAX_BRIDGE` tiles, placeable by the ordinary bridge rules).
use super::super::placement::{distance_squared, perimeter};
use super::plan::{Site, TIER_ROAD};

/// Roads to the gates wait for the second wall side.
const TIER_GATE_ROAD: u8 = 5;
use super::*;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// A gate node's outward direction; `None` for work points.
type Out = Option<(i32, i32)>;

/// Nodes (castle, work points, gates) joined into the network.
const MAX_NODES: usize = 64;
/// Tiles one route search may settle, independent of world size.
const EXPANSIONS: usize = 40_000;
/// Longest bridge a route may use, in tiles between the banks.
const MAX_BRIDGE: i32 = 14;
/// Extra route cost per new bridge tile: a bridge is a last resort over a long detour.
const BRIDGE_TILE_COST: u32 = 3;
/// Bank tiles between two of the domain's bridges at the least.
const BRIDGE_SPACING: i32 = 30;
/// Route cost of laying any new bridge, so roads walk along the bank to a
/// bridge the domain already has rather than span the river again nearby.
const NEW_BRIDGE_COST: u32 = 60;
/// Tiles beyond a gate that its road continues outside the wall.
const STUB: i32 = 3;

fn footprint_of(site: &Site) -> TileRect {
    let r = site.kind.rules();
    let (w, h) = if site.vertical {
        (r.depth, r.side)
    } else {
        (r.side, r.depth)
    };
    TileRect::from_xywh(site.tile.x, site.tile.y, w, h)
}

fn center_of(r: TileRect) -> TileCoord {
    TileCoord::new((r.min.x + r.max.x) / 2, (r.min.y + r.max.y) / 2)
}

/// Road, bridge and gate sites for the domain around `center`; `planned` is
/// the rest of the plan, whose footprints roads keep clear of.
pub(super) fn sites(
    w: &GameWorld,
    center: TileCoord,
    player: PlayerId,
    planned: &[Site],
) -> Vec<Site> {
    let Some(castle) = w
        .player_buildings_of(player, BuildingKind::Castle)
        .find(|b| b.footprint.contains(center))
    else {
        return Vec::new();
    };
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
    nodes.truncate(MAX_NODES);
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
    let mut net = Network {
        planned_spans: Vec::new(),
        w,
        player,
        avoid: &avoid,
        gate_tiles: &gate_tiles,
        goals: perimeter(castle.footprint).into_iter().collect(),
        out: Vec::new(),
        roads: BTreeSet::new(),
        bridge_ok: BTreeMap::new(),
        fixed: w
            .buildings()
            .filter(|b| b.player == player && b.kind == BuildingKind::Bridge && b.hp > 0)
            .filter_map(|b| b.bridge_span)
            .flat_map(|span| {
                let len = span.tiles().len() as u32;
                [(span.from, (span.to, len)), (span.to, (span.from, len))]
            })
            .collect(),
    };
    let mut connected = vec![center_of(castle.footprint)];
    // A node's edge: its whole perimeter, or for a gate the inner side only,
    // so a gate joins the network from inside the wall.
    let edge = |r: TileRect, out: Out| -> Vec<TileCoord> {
        let c = center_of(r);
        perimeter(r)
            .into_iter()
            .filter(|t| out.is_none_or(|o| (t.x - c.x) * o.0 + (t.y - c.y) * o.1 <= 0))
            .collect()
    };
    let mut remaining: Vec<(TileRect, Out, u8)> = nodes
        .into_iter()
        .enumerate()
        .map(|(i, (r, o))| (r, o, if i < local { TIER_ROAD } else { TIER_GATE_ROAD }))
        .collect();
    while !remaining.is_empty() {
        let nearest = |c: TileCoord| {
            connected
                .iter()
                .map(|&o| distance_squared(c, o))
                .min()
                .unwrap_or(0)
        };
        let (i, _) = remaining
            .iter()
            .enumerate()
            .min_by_key(|(_, (r, _, tier))| {
                let c = center_of(*r);
                (*tier, nearest(c), c.y, c.x)
            })
            .unwrap_or((0, &remaining[0]));
        let (node, out, tier) = remaining.swap_remove(i);
        let edge = edge(node, out);
        let starts: Vec<_> = edge
            .iter()
            .copied()
            .filter(|t| !avoid.contains(t))
            .collect();
        if net.route(&starts, tier) {
            connected.push(center_of(node));
            net.goals.extend(edge);
        }
    }
    // A road continues a few tiles outside each gate.
    for (c, (ox, oy)) in stubs {
        for n in 1..1 + STUB {
            let tile = TileCoord::new(c.x + ox * n, c.y + oy * n);
            net.lay(tile, TIER_GATE_ROAD);
        }
    }
    // Any own road still being built stays in the plan, route or not.
    let unfinished: Vec<_> = w
        .player_buildings_of(player, BuildingKind::Road)
        .filter(|b| b.hp > 0 && !b.is_complete() && in_domain(b.footprint))
        .map(|b| (b.id, b.footprint.min))
        .collect::<BTreeMap<_, _>>()
        .into_values()
        .collect();
    for tile in unfinished {
        if net.roads.insert(tile) {
            net.out.push(Site::new(BuildingKind::Road, tile, TIER_ROAD));
        }
    }
    net.out
}

struct Network<'a> {
    w: &'a GameWorld,
    player: PlayerId,
    avoid: &'a BTreeSet<TileCoord>,
    gate_tiles: &'a BTreeSet<TileCoord>,
    /// Tiles a route may end at: the joined nodes' edges and every road so far.
    goals: BTreeSet<TileCoord>,
    out: Vec<Site>,
    roads: BTreeSet<TileCoord>,
    bridge_ok: BTreeMap<(TileCoord, TileCoord), bool>,
    /// Own bridges already laid (even as foundations), by bank: the route
    /// uses them as they are instead of planning a parallel span.
    fixed: BTreeMap<TileCoord, (TileCoord, u32)>,
    /// Banks of spans this plan already lays.
    planned_spans: Vec<TileCoord>,
}

impl Network<'_> {
    /// Free open ground a road tile may be laid on.
    fn free(&self, tile: TileCoord) -> bool {
        !self.avoid.contains(&tile)
            && !self.gate_tiles.contains(&tile)
            && self.w.building_at(tile).is_none()
            && self.w.tile_passable(tile)
    }

    /// An own road, finished gate or bridge (a foundation too: re-planning
    /// must keep the span it already laid, not lay a parallel one).
    fn existing(&self, tile: TileCoord) -> bool {
        self.w
            .building_at(tile)
            .and_then(|id| self.w.building(id))
            .is_some_and(|b| {
                b.player == self.player
                    && (b.kind == BuildingKind::Road
                        || b.kind == BuildingKind::Bridge
                        || b.kind.is_gate() && b.is_complete())
            })
    }

    fn walkable(&self, tile: TileCoord) -> bool {
        self.free(tile) || self.existing(tile) || self.gate_tiles.contains(&tile)
    }

    fn lay(&mut self, tile: TileCoord, tier: u8) {
        if self.free(tile) && self.roads.insert(tile) {
            self.out.push(Site::new(BuildingKind::Road, tile, tier));
            self.goals.insert(tile);
        } else if self.existing(tile) {
            self.goals.insert(tile);
            // An unfinished road stays a deficit until it is complete.
            let unfinished = self
                .w
                .building_at(tile)
                .and_then(|id| self.w.building(id))
                .is_some_and(|b| b.kind == BuildingKind::Road && !b.is_complete());
            if unfinished && self.roads.insert(tile) {
                self.out.push(Site::new(BuildingKind::Road, tile, tier));
            }
        }
    }

    /// A bridge from bank `a` to bank `b`, checked once per pair.
    fn bridgeable(&mut self, a: TileCoord, b: TileCoord) -> bool {
        // A second span near a laid or planned one is never wanted: cross by
        // that one and walk the bank.
        let near =
            |t: TileCoord, o: TileCoord| (t.x - o.x).abs().max((t.y - o.y).abs()) <= BRIDGE_SPACING;
        if self
            .fixed
            .keys()
            .chain(self.planned_spans.iter())
            .any(|&f| near(f, a) || near(f, b))
        {
            return false;
        }
        let w = self.w;
        *self
            .bridge_ok
            .entry((a, b))
            .or_insert_with(|| w.bridge_site_valid(a, b))
    }

    /// Cheapest route from any of `starts` to the network; lays its tiles.
    /// A turn costs extra, so routes run in long straight lines that a crew
    /// lays as one order instead of a staircase of single tiles.
    fn route(&mut self, starts: &[TileCoord], tier: u8) -> bool {
        // A* toward the nearest network tile keeps far routes cheap.
        let aim = starts.first().and_then(|s| {
            self.goals
                .iter()
                .min_by_key(|g| (distance_squared(**g, *s), g.y, g.x))
                .copied()
        });
        let h = |t: TileCoord| -> u32 {
            aim.map_or(0, |a| ((t.x - a.x).abs() + (t.y - a.y).abs()) as u32)
        };
        const DIRS: [(i32, i32); 4] = [(0, -1), (-1, 0), (1, 0), (0, 1)];
        const TURN: u32 = 3;
        type State = (TileCoord, u8);
        // (cost, y, x, direction) keeps ties deterministic.
        let mut heap = BinaryHeap::new();
        let mut best: BTreeMap<State, (u32, Option<(State, bool)>)> = BTreeMap::new();
        for &t in starts.iter().filter(|t| self.walkable(**t)) {
            best.insert((t, 4), (0, None));
            heap.push(Reverse((h(t), 0_u32, t.y, t.x, 4_u8)));
        }
        let mut settled = 0;
        let mut found = None;
        while let Some(Reverse((_, cost, y, x, heading))) = heap.pop() {
            let tile = TileCoord::new(x, y);
            let here = (tile, heading);
            if best.get(&here).is_none_or(|(c, _)| *c < cost) {
                continue;
            }
            if self.goals.contains(&tile) {
                found = Some(here);
                break;
            }
            settled += 1;
            if settled > EXPANSIONS {
                break;
            }
            if let Some(&(other, len)) = self.fixed.get(&tile) {
                let d = match ((other.x - tile.x).signum(), (other.y - tile.y).signum()) {
                    (-1, _) => 1,
                    (1, _) => 2,
                    (_, 1) => 3,
                    _ => 0,
                };
                // Crossing a bridge already laid costs only its walk.
                let total = cost + len.max(1);
                if best.get(&(other, d)).is_none_or(|(c, _)| total < *c) {
                    best.insert((other, d), (total, Some((here, true))));
                    heap.push(Reverse((total + h(other), total, other.y, other.x, d)));
                }
            }
            for (d, (dx, dy)) in DIRS.into_iter().enumerate() {
                let d = d as u8;
                let turn = if heading != 4 && heading != d {
                    TURN
                } else {
                    0
                };
                let next = TileCoord::new(tile.x + dx, tile.y + dy);
                let (to, step, bridge) =
                    if self.walkable(next) && self.w.terrain.crossable(tile, next, self.w.config) {
                        (next, 1, false)
                    } else if !self.avoid.contains(&next)
                        && self.w.building_at(next).is_none()
                        && !self.w.tile_passable(next)
                    {
                        // Water (or other barrier) ahead: look for the far bank.
                        let far = (2..=MAX_BRIDGE).find_map(|k| {
                            let m = TileCoord::new(tile.x + dx * k, tile.y + dy * k);
                            if self.free(m) {
                                Some((k, m))
                            } else if self.w.tile_passable(m) || self.w.building_at(m).is_some() {
                                Some((0, m))
                            } else {
                                None
                            }
                        });
                        match far {
                            Some((k, m)) if k > 0 && self.bridgeable(tile, m) => {
                                (m, NEW_BRIDGE_COST + BRIDGE_TILE_COST * (k as u32 - 1), true)
                            }
                            _ => continue,
                        }
                    } else {
                        continue;
                    };
                let total = cost + step + turn;
                let state = (to, d);
                if best.get(&state).is_none_or(|(c, _)| total < *c) {
                    best.insert(state, (total, Some((here, bridge))));
                    heap.push(Reverse((total + h(to), total, to.y, to.x, d)));
                }
            }
        }
        let Some(goal) = found else {
            return false;
        };
        // Walk back to the start, laying roads and bridges.
        let mut at = goal;
        let mut spans = Vec::new();
        let mut tiles = vec![goal.0];
        while let Some((_, Some((prev, bridge)))) = best.get(&at).copied() {
            if bridge {
                spans.push((prev.0, at.0));
            }
            tiles.push(prev.0);
            at = prev;
        }
        let banks: BTreeSet<_> = spans.iter().flat_map(|(a, b)| [*a, *b]).collect();
        for tile in tiles.into_iter().filter(|t| !banks.contains(t)) {
            self.lay(tile, tier);
        }
        for (a, b) in spans {
            let (from, to) = if (a.x, a.y) <= (b.x, b.y) {
                (a, b)
            } else {
                (b, a)
            };
            self.out.push(Site {
                end: Some(to),
                ..Site::new(BuildingKind::Bridge, from, tier)
            });
            self.goals.extend([a, b]);
            self.planned_spans.extend([a, b]);
            // Later routes of this plan cross the planned span like a laid one.
            let len = (a.x - b.x).abs().max((a.y - b.y).abs()) as u32 + 1;
            self.fixed.insert(a, (b, len));
            self.fixed.insert(b, (a, len));
        }
        true
    }
}

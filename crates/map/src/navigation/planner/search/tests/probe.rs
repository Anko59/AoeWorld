//! One neighbour probe: the ground it reads, the step rule it applies, the
//! record it keeps and the cap it enforces.
use super::*;
use crate::navigation::{DIAGONAL_COST, ORTHOGONAL_COST, priority_cost};
use std::cell::RefCell;

type Answer<T> = Result<T, EnvironmentPageError>;
type Probed = Result<(), Terminal>;

fn tile(x: i32, y: i32) -> TileCoord {
    TileCoord::new(x, y)
}

fn step(from: TileCoord, to: TileCoord) -> u32 {
    if from.x == to.x || from.y == to.y {
        ORTHOGONAL_COST
    } else {
        DIAGONAL_COST
    }
}

/// Mirrors the leg loop: the tile being expanded is the active expansion.
fn probe(
    search: &mut SearchState,
    terrain: &dyn RouteTerrain,
    overlay: &ResourceOverlay,
    proof: Option<&ResourceOverlay>,
    from: (TileCoord, u64),
    offset: (i32, i32),
) -> Result<(), Terminal> {
    let active = ActiveExpansion {
        tile: from.0,
        cost: from.1,
        next_neighbor: 1,
    };
    search.active = Some(active);
    ExpansionContext::new(tile(40, 40), terrain, overlay, &|| false)
        .with_proof_overlay(proof)
        .expand_neighbor(search, active, offset)
}

/// A probe over open, level ground without a proof overlay.
fn level(search: &mut SearchState, from: (TileCoord, u64), offset: (i32, i32)) -> Probed {
    let overlay = ResourceOverlay::empty();
    probe(search, &Level, overlay, None, from, offset)
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Read {
    Walkable(TileCoord),
    Edge(TileCoord, TileCoord),
    Cost(TileCoord, TileCoord),
}

/// Open, level ground that lists every question it is asked.
#[derive(Default)]
struct Asked(RefCell<Vec<Read>>);

impl RouteTerrain for Asked {
    fn route_walkable(
        &self,
        _: &ResourceOverlay,
        at: TileCoord,
        _: &dyn Fn() -> bool,
    ) -> Answer<bool> {
        self.0.borrow_mut().push(Read::Walkable(at));
        Ok(true)
    }
    fn route_edge_clear(
        &self,
        from: TileCoord,
        to: TileCoord,
        _: &dyn Fn() -> bool,
    ) -> Answer<bool> {
        self.0.borrow_mut().push(Read::Edge(from, to));
        Ok(true)
    }
    fn route_step_cost(&self, from: TileCoord, to: TileCoord, _: &dyn Fn() -> bool) -> Answer<u32> {
        self.0.borrow_mut().push(Read::Cost(from, to));
        Ok(step(from, to))
    }
}

/// Open, level ground.
struct Level;

impl RouteTerrain for Level {
    fn route_walkable(
        &self,
        _: &ResourceOverlay,
        _: TileCoord,
        _: &dyn Fn() -> bool,
    ) -> Answer<bool> {
        Ok(true)
    }
    fn route_edge_clear(&self, _: TileCoord, _: TileCoord, _: &dyn Fn() -> bool) -> Answer<bool> {
        Ok(true)
    }
    fn route_step_cost(&self, from: TileCoord, to: TileCoord, _: &dyn Fn() -> bool) -> Answer<u32> {
        Ok(step(from, to))
    }
}

/// Every question the eight probes of one expansion ask on a provisional far
/// leg, where the planner passes the live overlay as its own proof snapshot.
fn provisional_expansion_reads(from: TileCoord) -> Vec<Read> {
    let overlay = ResourceOverlay::default();
    let ground = Asked::default();
    let mut search = SearchState::new(from, tile(40, 40));
    for index in 0..8 {
        let offset = neighbor_offset(index);
        probe(
            &mut search,
            &ground,
            &overlay,
            Some(&overlay),
            (from, 0),
            offset,
        )
        .unwrap();
        let next = tile(from.x + offset.0, from.y + offset.1);
        assert!(search.records.contains_key(&next), "{next:?} is open");
    }
    ground.0.into_inner()
}

#[test]
fn a_provisional_expansion_reads_each_side_of_each_step_and_nothing_else() {
    let from = tile(8, 8);
    let mut expected = Vec::new();
    for index in 0..8 {
        let (dx, dy) = neighbor_offset(index);
        let next = tile(from.x + dx, from.y + dy);
        expected.extend([
            Read::Walkable(next),
            Read::Edge(from, next),
            Read::Edge(next, from),
            Read::Cost(from, next),
        ]);
        if dx != 0 && dy != 0 {
            // A diagonal step also needs both corner tiles open (they are two
            // of the eight neighbours) and reachable from either end.
            for corner in [tile(next.x, from.y), tile(from.x, next.y)] {
                expected.extend([Read::Edge(from, corner), Read::Edge(next, corner)]);
            }
        }
    }
    expected.sort();
    expected.dedup();
    assert_eq!(expected.len(), 40);
    let mut asked = provisional_expansion_reads(from);
    asked.sort();
    asked.dedup();
    assert_eq!(asked, expected);
}

#[test]
fn a_provisional_expansion_does_not_repeat_checks_against_the_same_overlay() {
    // With the live overlay as the proof snapshot, a diagonal probe checked
    // its forward corners twice and its reverse corners twice: 21 questions
    // where 13 differ in overlay or direction, 104 for the eight probes.
    let asked = provisional_expansion_reads(tile(8, 8));
    let count = |wanted: fn(&Read) -> bool| asked.iter().filter(|read| wanted(read)).count();
    let walkable = count(|read| matches!(read, Read::Walkable(_)));
    let edges = count(|read| matches!(read, Read::Edge(..)));
    let costs = count(|read| matches!(read, Read::Cost(..)));
    assert_eq!(costs, 8, "one step cost for each accepted neighbour");
    assert!(
        asked.len() <= 4 * 12 + 4 * 4,
        "one expansion asked {} questions ({walkable} tiles, {edges} edges, {costs} costs); \
         64 remain once the repeated checks are skipped",
        asked.len()
    );
}

/// Ground with faults on otherwise open, level land. Water is walkable only
/// under a bridge deck, as on a map.
#[derive(Clone, Default)]
struct Faults {
    water: Vec<TileCoord>,
    edges: Vec<(TileCoord, TileCoord)>,
}

impl Faults {
    fn walkable(&self, overlay: &ResourceOverlay, at: TileCoord) -> bool {
        (!self.water.contains(&at) || overlay.bridge_connections(at).is_some())
            && !overlay.blocks_tile(at)
    }

    fn edge(&self, overlay: &ResourceOverlay, from: TileCoord, to: TileCoord) -> bool {
        overlay
            .bridge_edge(from, to)
            .unwrap_or(!self.edges.contains(&(from, to)))
    }

    /// A diagonal step may not cut a corner: both corner tiles are open and
    /// reachable from `from`, unless a bridge deck decides the step itself.
    fn corners(&self, overlay: &ResourceOverlay, from: TileCoord, to: TileCoord) -> bool {
        overlay.bridge_edge(from, to).unwrap_or_else(|| {
            [tile(to.x, from.y), tile(from.x, to.y)]
                .into_iter()
                .all(|corner| self.walkable(overlay, corner) && self.edge(overlay, from, corner))
        })
    }

    /// The step rule of `expand_neighbor`, written out. The step is legal on
    /// the live overlay; under a proof snapshot it can also be walked back
    /// there, and a diagonal one cuts no corner in either overlay or direction.
    fn allows(
        &self,
        live: &ResourceOverlay,
        proof: Option<&ResourceOverlay>,
        from: TileCoord,
        to: TileCoord,
    ) -> bool {
        let diagonal = from.x != to.x && from.y != to.y;
        let forward = self.walkable(live, to)
            && self.edge(live, from, to)
            && (!diagonal || self.corners(live, from, to));
        forward
            && proof.is_none_or(|snapshot| {
                self.walkable(snapshot, to)
                    && self.edge(snapshot, to, from)
                    && (!diagonal
                        || (self.corners(live, to, from)
                            && self.corners(snapshot, from, to)
                            && self.corners(snapshot, to, from)))
            })
    }
}

impl RouteTerrain for Faults {
    fn route_walkable(
        &self,
        overlay: &ResourceOverlay,
        at: TileCoord,
        _: &dyn Fn() -> bool,
    ) -> Answer<bool> {
        Ok(self.walkable(overlay, at))
    }
    fn route_edge_clear(
        &self,
        from: TileCoord,
        to: TileCoord,
        _: &dyn Fn() -> bool,
    ) -> Answer<bool> {
        Ok(!self.edges.contains(&(from, to)))
    }
    fn route_step_cost(&self, from: TileCoord, to: TileCoord, _: &dyn Fn() -> bool) -> Answer<u32> {
        Ok(step(from, to))
    }
}

const CENTRE: TileCoord = TileCoord::new(8, 8);

/// The centre's eight neighbours.
fn ring() -> Vec<TileCoord> {
    (0..8)
        .map(neighbor_offset)
        .map(|(dx, dy)| tile(CENTRE.x + dx, CENTRE.y + dy))
        .collect()
}

/// Every directed step between two tiles of the centre's 3 x 3 block.
fn block_edges() -> Vec<(TileCoord, TileCoord)> {
    let mut block = ring();
    block.push(CENTRE);
    let mut edges = Vec::new();
    for from in &block {
        for to in &block {
            if from != to && (from.x - to.x).abs() <= 1 && (from.y - to.y).abs() <= 1 {
                edges.push((*from, *to));
            }
        }
    }
    edges
}

/// One overlay per bridge layout around the centre: none, decks through the
/// centre, decks beside it, and decks that start on it.
fn bridged() -> Vec<ResourceOverlay> {
    let spans = [
        ((7, 8), (10, 8)),
        ((8, 7), (8, 10)),
        ((7, 7), (10, 10)),
        ((9, 6), (9, 10)),
        ((6, 9), (10, 9)),
        ((8, 8), (11, 8)),
        ((8, 8), (11, 11)),
        ((5, 5), (8, 8)),
        ((6, 10), (10, 6)),
    ];
    let mut overlays = vec![ResourceOverlay::default()];
    for (from, to) in spans {
        let mut overlay = ResourceOverlay::default();
        let span = aoe_core::BridgeSpan::new(tile(from.0, from.1), tile(to.0, to.1)).unwrap();
        overlay.add_bridge(span).unwrap();
        overlays.push(overlay);
    }
    overlays
}

/// Probes all eight neighbours of the centre and compares each decision with
/// the rule. Returns how many probes were accepted and how many refused.
fn check_expansion(
    ground: &Faults,
    live: &ResourceOverlay,
    proof: Option<&ResourceOverlay>,
) -> (u32, u32) {
    let (mut accepted, mut refused) = (0, 0);
    for index in 0..8 {
        let offset = neighbor_offset(index);
        let next = tile(CENTRE.x + offset.0, CENTRE.y + offset.1);
        let mut search = SearchState::new(CENTRE, tile(40, 40));
        probe(&mut search, ground, live, proof, (CENTRE, 7), offset).unwrap();
        let expected = ground
            .allows(live, proof, CENTRE, next)
            .then_some(SearchRecord {
                cost: 7 + u64::from(step(CENTRE, next)),
                parent: Some(CENTRE),
            });
        assert_eq!(
            search.records.get(&next).copied(),
            expected,
            "step {offset:?}, water {:?}, edges {:?}, same overlay as proof: {:?}",
            ground.water,
            ground.edges,
            proof.map(|snapshot| std::ptr::eq(snapshot, live)),
        );
        let entries = 1 + usize::from(expected.is_some());
        assert_eq!(
            (search.records.len(), search.open.len()),
            (entries, entries)
        );
        if expected.is_some() {
            accepted += 1;
        } else {
            refused += 1;
        }
    }
    (accepted, refused)
}

#[test]
fn the_step_rule_holds_for_every_fault_bridge_and_proof_overlay() {
    let (ring, edges, bridges) = (ring(), block_edges(), bridged());
    assert_eq!(edges.len(), 40);
    // Each fault alone and every pair of faults: a dry or wet neighbour, a
    // blocked directed step.
    let mut grounds = vec![Faults::default()];
    let single: Vec<Faults> = ring
        .iter()
        .map(|at| Faults {
            water: vec![*at],
            edges: Vec::new(),
        })
        .chain(edges.iter().map(|edge| Faults {
            water: Vec::new(),
            edges: vec![*edge],
        }))
        .collect();
    for (index, first) in single.iter().enumerate() {
        grounds.push(first.clone());
        for second in &single[index + 1..] {
            let mut both = first.clone();
            both.water.extend(&second.water);
            both.edges.extend(&second.edges);
            grounds.push(both);
        }
    }
    let (mut accepted, mut refused) = (0, 0);
    let mut tally = |(yes, no): (u32, u32)| {
        accepted += yes;
        refused += no;
    };
    for ground in &grounds {
        let live = &bridges[0];
        tally(check_expansion(ground, live, None));
        tally(check_expansion(ground, live, Some(live)));
        // A snapshot that is another overlay with the same content decides
        // exactly as the live overlay itself.
        let same = ResourceOverlay::default();
        tally(check_expansion(ground, live, Some(&same)));
    }
    // Buildings and bridge decks that only one of the two overlays holds.
    let mut overlays = bridges;
    for at in &ring {
        let mut overlay = ResourceOverlay::default();
        overlay
            .add_footprint(aoe_core::TileRect::from_xywh(at.x, at.y, 1, 1))
            .unwrap();
        overlays.push(overlay);
    }
    for ground in grounds.iter().take(1 + single.len()) {
        for live in &overlays {
            tally(check_expansion(ground, live, None));
            tally(check_expansion(ground, live, Some(live)));
            for snapshot in &overlays {
                tally(check_expansion(ground, live, Some(snapshot)));
            }
        }
    }
    // The table is only worth its name if both answers are well represented.
    assert!(accepted > 10_000, "{accepted} accepted");
    assert!(refused > 10_000, "{refused} refused");
}

#[test]
fn a_probe_keeps_the_cheaper_record_and_queues_only_an_improvement() {
    let mut search = SearchState::new(tile(0, 0), tile(40, 40));
    let next = tile(11, 10);
    let record = |search: &SearchState| search.records.get(&next).copied();
    let sizes = |search: &SearchState| (search.records.len(), search.open.len());
    let kept = |cost, parent: TileCoord| {
        Some(SearchRecord {
            cost,
            parent: Some(parent),
        })
    };
    // Unknown: recorded and queued.
    level(&mut search, (tile(10, 10), 5_000), (1, 0)).unwrap();
    assert_eq!(record(&search), kept(6_024, tile(10, 10)));
    assert_eq!(sizes(&search), (2, 2));
    // Dearer, then equal: the known record stays and nothing is queued.
    level(&mut search, (tile(12, 10), 6_000), (-1, 0)).unwrap();
    level(&mut search, (tile(11, 11), 5_000), (0, -1)).unwrap();
    assert_eq!(record(&search), kept(6_024, tile(10, 10)));
    assert_eq!(sizes(&search), (2, 2));
    // Cheaper: the record is replaced, and the superseded node stays queued.
    level(&mut search, (tile(12, 11), 4_000), (-1, -1)).unwrap();
    assert_eq!(record(&search), kept(5_448, tile(12, 11)));
    assert_eq!(sizes(&search), (2, 3));
}

#[test]
fn the_frontier_orders_by_total_then_larger_cost_then_row_then_column() {
    let node = |total, cost, x, y| OpenNode::new(total, priority_cost(cost), tile(x, y));
    let ordered = [
        node(10, 4, 9, 9),
        node(20, 9, 7, 7),
        node(20, 5, 1, 1),
        node(20, 5, 9, 1),
        node(20, 5, 0, 2),
        node(20, 3, 0, 0),
        node(21, 21, 0, 0),
    ];
    for (index, first) in ordered.iter().enumerate() {
        for (other, second) in ordered.iter().enumerate() {
            assert_eq!(first.cmp(second), index.cmp(&other), "{first:?} {second:?}");
        }
    }
}

/// Distinct sites, each with an unknown tile to its east.
fn site(index: i32) -> TileCoord {
    tile(4 * (index % 1_024), 8 + 4 * (index / 1_024))
}

/// Probes the tile east of a site for the first time, at a high cost.
fn fresh(search: &mut SearchState, index: i32) -> Probed {
    level(search, (site(index), 100_000), (1, 0))
}

/// Probes that tile again from its own east, far cheaper.
fn improve(search: &mut SearchState, index: i32) -> Probed {
    let from = tile(site(index).x + 2, site(index).y);
    level(search, (from, 0), (-1, 0))
}

#[test]
fn the_node_cap_counts_a_new_record_twice_and_an_improvement_once() {
    let mut search = SearchState::new(tile(0, 0), tile(40, 40));
    let retained = |search: &SearchState| search.records.len() + search.open.len() + 1;
    // A record and a frontier node for each, beside the origin's own pair and
    // the active expansion: one entry short of the cap.
    let filled = (MAX_ROUTE_PLANNER_NODES as i32 - 4) / 2;
    for index in 0..filled {
        fresh(&mut search, index).unwrap();
    }
    assert_eq!(retained(&search), MAX_ROUTE_PLANNER_NODES - 1);
    let sizes = (search.records.len(), search.open.len());
    // A new record needs two entries and there is room for one.
    assert_eq!(fresh(&mut search, filled), Err(Terminal::SearchLimit));
    assert_eq!((search.records.len(), search.open.len()), sizes);
    // An improvement needs one: its record exists.
    assert_eq!(improve(&mut search, 0), Ok(()));
    assert_eq!(retained(&search), MAX_ROUTE_PLANNER_NODES);
    assert_eq!(improve(&mut search, 1), Err(Terminal::SearchLimit));
    // A probe that keeps nothing is never refused, even at the cap.
    assert_eq!(improve(&mut search, 0), Ok(()));
    assert_eq!(retained(&search), MAX_ROUTE_PLANNER_NODES);
    assert_eq!(
        (search.records.len(), search.open.len()),
        (sizes.0, sizes.1 + 1)
    );
}

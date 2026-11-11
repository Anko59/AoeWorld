use super::*;

struct Cliff;
impl RouteTerrain for Cliff {
    fn route_walkable(
        &self,
        overlay: &ResourceOverlay,
        tile: TileCoord,
        _: &dyn Fn() -> bool,
    ) -> Result<bool, EnvironmentPageError> {
        Ok((0..2).contains(&tile.x) && (0..2).contains(&tile.y) && !overlay.blocks_tile(tile))
    }
    fn route_edge_clear(
        &self,
        from: TileCoord,
        to: TileCoord,
        _: &dyn Fn() -> bool,
    ) -> Result<bool, EnvironmentPageError> {
        let height = |tile: TileCoord| match (tile.x, tile.y) {
            (0, 0) => 1_i32,
            (1, 1) => 2,
            _ => 0,
        };
        Ok((height(from) - height(to)).abs() <= 1)
    }
    fn route_step_cost(
        &self,
        _: TileCoord,
        _: TileCoord,
        _: &dyn Fn() -> bool,
    ) -> Result<u32, EnvironmentPageError> {
        Ok(1_024)
    }
}

#[test]
fn provisional_progress_cannot_take_a_one_way_cliff_step() {
    let overlay = ResourceOverlay::default();
    let from = TileCoord::new(0, 0);
    let to = TileCoord::new(1, 1);
    for (proof, allowed) in [(None, true), (Some(&overlay), false)] {
        let mut search = SearchState::new(from, to);
        ExpansionContext::new(to, &Cliff, &overlay, &|| false)
            .with_proof_overlay(proof)
            .expand_neighbor(
                &mut search,
                ActiveExpansion {
                    tile: from,
                    cost: 0,
                    next_neighbor: 0,
                },
                (1, 1),
            )
            .unwrap();
        assert_eq!(search.records.contains_key(&to), allowed);
    }
}

#[test]
fn provisional_progress_stays_inside_the_proof_collision_snapshot() {
    let current = ResourceOverlay::default();
    let mut snapshot = ResourceOverlay::default();
    snapshot
        .add_footprint(aoe_core::TileRect::from_xywh(1, 0, 1, 1))
        .unwrap();
    let from = TileCoord::new(0, 0);
    let to = TileCoord::new(1, 0);
    let mut search = SearchState::new(from, to);
    ExpansionContext::new(to, &Cliff, &current, &|| false)
        .with_proof_overlay(Some(&snapshot))
        .expand_neighbor(
            &mut search,
            ActiveExpansion {
                tile: from,
                cost: 0,
                next_neighbor: 0,
            },
            (1, 0),
        )
        .unwrap();
    assert!(!search.records.contains_key(&to));
}

/// The planner searches through its per-poll memo. A tile free in the live
/// overlay but blocked in the proof snapshot must stay out of the search:
/// the memo must not answer the snapshot's question with the live answer.
#[test]
fn memoized_search_still_reads_the_proof_snapshot() {
    let current = ResourceOverlay::default();
    let mut snapshot = ResourceOverlay::default();
    snapshot
        .add_footprint(aoe_core::TileRect::from_xywh(1, 0, 1, 1))
        .unwrap();
    let from = TileCoord::new(0, 0);
    let to = TileCoord::new(1, 0);
    let memo = super::super::memo::MemoTerrain::new(&Cliff, &current);
    let mut search = SearchState::new(from, to);
    ExpansionContext::new(to, &memo, &current, &|| false)
        .with_proof_overlay(Some(&snapshot))
        .expand_neighbor(
            &mut search,
            ActiveExpansion {
                tile: from,
                cost: 0,
                next_neighbor: 0,
            },
            (1, 0),
        )
        .unwrap();
    assert!(!search.records.contains_key(&to));
}

/// Counts every terrain read; a dominated neighbour must need none.
struct Counting(std::cell::Cell<u32>);
impl RouteTerrain for Counting {
    fn route_walkable(
        &self,
        _: &ResourceOverlay,
        _: TileCoord,
        _: &dyn Fn() -> bool,
    ) -> Result<bool, EnvironmentPageError> {
        self.0.set(self.0.get() + 1);
        Ok(true)
    }
    fn route_edge_clear(
        &self,
        _: TileCoord,
        _: TileCoord,
        _: &dyn Fn() -> bool,
    ) -> Result<bool, EnvironmentPageError> {
        self.0.set(self.0.get() + 1);
        Ok(true)
    }
    fn route_step_cost(
        &self,
        _: TileCoord,
        _: TileCoord,
        _: &dyn Fn() -> bool,
    ) -> Result<u32, EnvironmentPageError> {
        self.0.set(self.0.get() + 1);
        Ok(0)
    }
}

#[test]
fn a_dominated_neighbour_skips_terrain_but_still_observes_cancellation() {
    let overlay = ResourceOverlay::default();
    let (origin, active) = (TileCoord::new(0, 0), TileCoord::new(1, 0));
    let expansion = ActiveExpansion {
        tile: active,
        cost: 0,
        next_neighbor: 0,
    };
    let terrain = Counting(std::cell::Cell::new(0));
    let mut search = SearchState::new(origin, TileCoord::new(5, 0));
    let before = search.clone();
    ExpansionContext::new(TileCoord::new(5, 0), &terrain, &overlay, &|| false)
        .expand_neighbor(&mut search, expansion, (-1, 0))
        .unwrap();
    assert_eq!(terrain.0.get(), 0, "an equal-cost record cannot improve");
    assert_eq!(search, before);
    let cancelled = ExpansionContext::new(TileCoord::new(5, 0), &terrain, &overlay, &|| true)
        .expand_neighbor(&mut search, expansion, (-1, 0));
    assert!(matches!(
        cancelled,
        Err(Terminal::Environment(EnvironmentPageError::Cancelled))
    ));
    // A zero-cost step to a costlier record still reads and improves it.
    search.records.get_mut(&origin).unwrap().cost = 1;
    ExpansionContext::new(TileCoord::new(5, 0), &terrain, &overlay, &|| false)
        .expand_neighbor(&mut search, expansion, (-1, 0))
        .unwrap();
    assert!(terrain.0.get() > 0);
    assert_eq!(search.records[&origin].cost, 0);
}

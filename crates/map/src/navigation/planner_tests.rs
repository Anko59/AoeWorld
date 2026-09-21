use super::*;

#[test]
fn expansion_rejects_an_oversize_retained_frontier() {
    let terrain = MapChunkGenerator::new([0; 32], 0, 4);
    let overlay = ResourceOverlay::default();
    let origin = TileCoord::new(1, 1);
    let mut search = SearchState::new(origin, TileCoord::new(2, 2));
    for index in 0..(MAX_ROUTE_PLANNER_NODES - 3) {
        search
            .scores
            .insert(TileCoord::new(10 + index as i32, 0), 1);
    }
    assert_eq!(
        search.open.len() + search.scores.len(),
        MAX_ROUTE_PLANNER_NODES - 1
    );
    let planner = RoutePlanner::new(origin, TileCoord::new(2, 2), 4_096);
    assert_eq!(
        planner.expand(&terrain, &overlay, &|| false, &mut search, origin, 0),
        Err(Terminal::SearchLimit)
    );
    assert_eq!(
        search.open.len() + search.scores.len(),
        MAX_ROUTE_PLANNER_NODES - 1
    );
}

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

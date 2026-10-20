use super::*;
use aoe_core::{Tick, TileRect};
use aoe_protocol::FogState;

#[test]
fn no_authoritative_view_starts_black() {
    let fog = Fog::default();
    assert!(!fog.disabled());
    assert_eq!(fog.brightness([20.5, 20.5]), 0.0);
}

#[test]
fn tile_shading_distinguishes_black_fog_and_visible_and_softens_edges() {
    let mut fog = Fog::default();
    fog.apply(FogState {
        revision: 1,
        tick: Tick(0),
        region: TileRect::from_xywh(0, 0, 3, 1),
        disabled: false,
        tiles: vec![0, 1, 2],
        overview: vec![],
    });
    assert_eq!(fog.state([0, 0]), 0);
    assert_eq!(fog.state([1, 0]), 1);
    assert_eq!(fog.state([2, 0]), 2);
    assert_eq!(fog.brightness([0.5, 0.5]), 0.0);
    assert_eq!(fog.brightness([1.5, 0.5]), 0.45);
    assert_eq!(fog.brightness([2.5, 0.5]), 1.0);
    assert!((0.45..1.0).contains(&fog.brightness([2.0, 0.5])));
}

#[test]
fn a_fog_disabled_scenario_is_fully_lit() {
    let mut fog = Fog::default();
    fog.apply(FogState {
        revision: 1,
        tick: Tick(0),
        region: TileRect::from_xywh(0, 0, 1, 1),
        disabled: true,
        tiles: vec![],
        overview: vec![],
    });
    assert_eq!(fog.brightness([1_000_000.0, 0.0]), 1.0);
}

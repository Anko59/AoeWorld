use super::*;
use aoe_core::{Tick, TileRect};
use aoe_protocol::FogState;

#[test]
fn repeated_distant_fog_views_replace_the_old_region_with_a_fixed_payload_budget() {
    let mut fog = Fog::default();
    for cycle in 0..3 {
        for index in 0..64 {
            let x = (index % 8) * 2000;
            let y = (index / 8) * 2000;
            assert!(fog.apply(FogState {
                revision: (cycle * 64 + index + 1) as u64,
                tick: Tick(0),
                region: TileRect::from_xywh(x, y, 512, 512),
                disabled: false,
                tiles: vec![2; 512 * 512],
                overview: (0..65536).map(|pixel| (pixel as u16, 1)).collect(),
            }));
            assert_eq!(fog.state([x, y]), 2);
            if index > 0 {
                let previous = index - 1;
                assert_eq!(fog.state([(previous % 8) * 2000, (previous / 8) * 2000]), 0);
            }
            assert_eq!(fog.resident_bytes(), 512 * 1024);
        }
    }
    fog.clear();
    assert_eq!(fog.resident_bytes(), 0);
}

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

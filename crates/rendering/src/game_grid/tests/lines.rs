use super::*;
use wasm_bindgen_test::wasm_bindgen_test;

fn line(depth: f64) -> GridLine {
    GridLine {
        ends: [
            ScreenPoint { x: 0.0, y: 1.0 },
            ScreenPoint { x: 7.0, y: 1.0 },
        ],
        depths: [depth; 2],
    }
}

#[wasm_bindgen_test]
fn grid_lifts_clear_of_terrain_but_stays_below_selection_rings() {
    for zoom in [0.25, 1.0, 3.0] {
        let lift = grid_depth_lift(zoom);
        assert!(lift > 0.5 / (ISO_TILE_HEIGHT / 2.0 * zoom), "zoom {zoom}");
        assert!(lift < 1.4, "zoom {zoom} stays under the 1.4 ring lift");
    }
}

#[wasm_bindgen_test]
fn grid_pixels_show_on_terrain_and_hide_behind_nearer_objects() {
    let (width, ground, object) = (8, 10.0, 11.0);
    let mut color = vec![100u8; 8 * 4 * 3];
    let mut depth = vec![ground; 8 * 3];
    // An object (building, unit, shadow) is frontmost on pixels 2 and 3.
    depth[8 + 2] = object;
    depth[8 + 3] = object;
    raster_grid(
        &[line(ground + 0.1)],
        [0, 0, 8, 3],
        width,
        &mut color,
        &depth,
    );
    let red = |x: usize| color[(8 + x) * 4];
    assert!(
        red(0) < 100 && red(5) < 100,
        "terrain pixels carry the grid"
    );
    assert_eq!((red(2), red(3)), (100, 100), "objects stay on top");
    assert_eq!(color[0], 100, "other rows are untouched");
}

#[wasm_bindgen_test]
fn grid_raster_stays_inside_the_damage_region() {
    let mut color = vec![100u8; 8 * 4 * 3];
    let depth = vec![0.0; 8 * 3];
    raster_grid(&[line(1.0)], [2, 0, 5, 3], 8, &mut color, &depth);
    let touched: Vec<usize> = (0..8).filter(|x| color[(8 + x) * 4] != 100).collect();
    assert_eq!(touched, [2, 3, 4]);
}

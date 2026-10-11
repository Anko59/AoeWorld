use super::*;
use wasm_bindgen::JsCast;

fn camera() -> SceneCamera {
    SceneCamera {
        center: [35_000.0; 2],
        zoom: 1.0,
        viewport: [128.0; 2],
        focus_elevation_meters: 800.0,
    }
}

fn projection(camera: SceneCamera) -> Camera {
    Camera {
        center: camera.center,
        zoom: camera.zoom,
        viewport: camera.viewport,
        focus_elevation_meters: camera.focus_elevation_meters,
    }
}

fn tile(x: i32, y: i32, level: i16) -> SceneTerrain {
    SceneTerrain {
        appearance: None,
        position: [f64::from(x) + 0.5, f64::from(y) + 0.5],
        material: 0,
        elevation_meters: f64::from(level),
        surface: SceneTerrainSurface::flat(f64::from(level)),
    }
}

fn field(side: i32, raised: Option<[i32; 2]>) -> Vec<SceneTerrain> {
    (0..side * side)
        .map(|index| {
            let [x, y] = [index % side, index / side];
            tile(x, y, i16::from(raised == Some([x, y])))
        })
        .collect()
}

fn segments(terrain: &[SceneTerrain], camera: SceneCamera) -> Vec<[ScreenPoint; 2]> {
    game_grid::grid_segments(&projected_surface_triangles(terrain, camera), camera.zoom)
}

fn has(segments: &[[ScreenPoint; 2]], ends: [ScreenPoint; 2]) -> usize {
    let near =
        |a: ScreenPoint, b: ScreenPoint| (a.x - b.x).abs() < 1e-6 && (a.y - b.y).abs() < 1e-6;
    segments
        .iter()
        .filter(|[start, end]| {
            (near(*start, ends[0]) && near(*end, ends[1]))
                || (near(*start, ends[1]) && near(*end, ends[0]))
        })
        .count()
}

#[wasm_bindgen_test]
fn grid_draws_every_tile_border_once_and_no_diagonal() {
    let camera = SceneCamera {
        center: [1.5; 2],
        zoom: 1.0,
        viewport: [512.0, 256.0],
        focus_elevation_meters: 0.0,
    };
    let lines = segments(&field(3, None), camera);
    let at = |x: i32, y: i32| {
        projection(camera).world_to_screen_at_height([f64::from(x), f64::from(y)], 0.0)
    };
    // Borders belong to the tile on their positive side: lines 0, 1 and 2.
    for line in 0..3 {
        for along in 0..3 {
            assert_eq!(has(&lines, [at(line, along), at(line, along + 1)]), 1);
            assert_eq!(has(&lines, [at(along, line), at(along + 1, line)]), 1);
        }
    }
    assert_eq!(lines.len(), 18);
    // A flat tile border is one tile diamond edge: 64 by 32 pixels at zoom 1.
    for [start, end] in &lines {
        assert_eq!((start.x - end.x).abs(), 64.0);
        assert_eq!((start.y - end.y).abs(), 32.0);
    }
}

#[wasm_bindgen_test]
fn grid_follows_terrain_height() {
    let camera = SceneCamera {
        center: [1.5; 2],
        zoom: 1.0,
        viewport: [512.0, 256.0],
        focus_elevation_meters: 0.0,
    };
    let lines = segments(&field(3, Some([1, 1])), camera);
    let at =
        |x: f64, y: f64, height: f64| projection(camera).world_to_screen_at_height([x, y], height);
    // The raised tile draws its own two borders one level up the screen.
    assert_eq!(has(&lines, [at(1.0, 1.0, 1.0), at(1.0, 2.0, 1.0)]), 1);
    assert_eq!(has(&lines, [at(1.0, 1.0, 1.0), at(2.0, 1.0, 1.0)]), 1);
    assert_eq!(has(&lines, [at(1.0, 1.0, 0.0), at(1.0, 2.0, 0.0)]), 0);
    assert!(at(1.0, 1.0, 1.0).y < at(1.0, 1.0, 0.0).y);
    // Its neighbours keep their borders on the ground.
    assert_eq!(has(&lines, [at(2.0, 1.0, 0.0), at(2.0, 2.0, 0.0)]), 1);
}

#[wasm_bindgen_test]
fn grid_thins_out_instead_of_filling_a_distant_view() {
    let terrain = field(32, None);
    let mut camera = SceneCamera {
        center: [16.0; 2],
        zoom: 1.0,
        viewport: [512.0, 256.0],
        focus_elevation_meters: 0.0,
    };
    let near = segments(&terrain, camera).len();
    assert!(near > 0);
    for (zoom, step) in [(0.25, 2.0), (0.2, 2.0), (0.1, 4.0), (0.04, 8.0)] {
        camera.zoom = zoom;
        let lines = segments(&terrain, camera);
        assert!(!lines.is_empty(), "zoom {zoom} keeps a grid");
        let mut kept = [
            std::collections::BTreeSet::new(),
            std::collections::BTreeSet::new(),
        ];
        for [start, end] in &lines {
            let world =
                [start, end].map(|point| projection(camera).screen_to_world_at_height(*point, 0.0));
            let axis = usize::from((world[0][0] - world[1][0]).abs() > 0.5);
            let line = world[0][axis].round();
            assert!((world[1][axis] - line).abs() < 1e-6);
            assert_eq!(line.rem_euclid(step), 0.0, "zoom {zoom} line {line}");
            kept[axis].insert(line as i64);
        }
        // Neighbouring kept lines are `step` tiles, at least 16 screen pixels, apart.
        for lines in &kept {
            assert!(lines.len() > 1);
            for pair in lines.iter().collect::<Vec<_>>().windows(2) {
                assert_eq!((pair[1] - pair[0]) as f64, step);
            }
        }
        assert!(step * 64.0 * zoom >= 16.0);
    }
    // The overlay is one translucent pixel wide, never an opaque fill.
    let sprites = game_grid::grid_sprites(&segments(&terrain, camera), camera.viewport);
    assert!(!sprites.is_empty());
    for pair in sprites.chunks_exact(2) {
        assert_eq!(pair[0].color[3], -2.0);
        assert!(pair[0].uv[3] > 0.25 && pair[0].uv[3] < 0.75);
        let pixels = |clip: f32| f64::from(clip) * camera.viewport[1] / 2.0;
        let height = pixels(pair[0].position[1]) - pixels(pair[0].radius[1]);
        assert!((height - 1.0).abs() < 1e-3, "line is {height} px thick");
    }
}

#[wasm_bindgen_test]
fn grid_lines_keep_sixteen_pixels_of_perpendicular_spacing_at_the_minimum_zoom() {
    // The zoom control clamps at 0.25, the worst case for line spacing; a
    // tile's screen height alone overstates the real distance in the 2:1
    // projection, where neighbouring parallel lines sit closer together.
    let terrain = field(8, None);
    let camera = SceneCamera {
        center: [4.0; 2],
        zoom: 0.25,
        viewport: [512.0, 256.0],
        focus_elevation_meters: 0.0,
    };
    let lines = segments(&terrain, camera);
    assert!(!lines.is_empty(), "the minimum zoom still draws a grid");
    // Perpendicular screen distance per world tile between neighbouring
    // parallel lines of one family: the tile diamond's altitude,
    // W * H / sqrt(W^2 + H^2), scaled by the zoom.
    let per_tile = aoe_core::ISO_TILE_WIDTH * aoe_core::ISO_TILE_HEIGHT
        / (aoe_core::ISO_TILE_WIDTH * aoe_core::ISO_TILE_WIDTH
            + aoe_core::ISO_TILE_HEIGHT * aoe_core::ISO_TILE_HEIGHT)
            .sqrt();
    let mut kept = [
        std::collections::BTreeSet::new(),
        std::collections::BTreeSet::new(),
    ];
    for [start, end] in &lines {
        let world =
            [start, end].map(|point| projection(camera).screen_to_world_at_height(*point, 0.0));
        let axis = usize::from((world[0][0] - world[1][0]).abs() > 0.5);
        let line = world[0][axis].round();
        assert!((world[1][axis] - line).abs() < 1e-6);
        kept[axis].insert(line as i64);
    }
    for lines in &kept {
        assert!(lines.len() > 1);
        for pair in lines.iter().collect::<Vec<_>>().windows(2) {
            let spacing = pair[1].abs_diff(*pair[0]) as f64 * per_tile * camera.zoom;
            assert!(
                spacing >= 16.0,
                "kept lines {pair:?} are {spacing} px apart on screen"
            );
        }
    }
}

#[wasm_bindgen_test]
fn canvas_live_source_grid_has_pixels_and_toggle_off_removes_them() {
    let document = web_sys::window().unwrap().document().unwrap();
    let canvas = document
        .create_element("canvas")
        .unwrap()
        .dyn_into::<HtmlCanvasElement>()
        .unwrap();
    canvas.set_width(128);
    canvas.set_height(128);
    let context = context(&canvas).unwrap();
    let mut renderer = GameRenderer::Canvas {
        canvas: canvas.clone(),
        context: context.clone(),
        atlas: [None, None, None],
        source_atlas: Vec::new(),
        presentation: CanvasPresentation::new(128, 128),
    };
    let camera = camera();
    let terrain = [tile(35_000, 35_000, 800)];
    let surfaces = projected_surface_triangles(&terrain, camera);
    // The border from the screen centre to (128, 96) passes through (96, 80).
    let pixel = || {
        context
            .get_image_data(95.0, 79.0, 2.0, 2.0)
            .unwrap()
            .data()
            .0
    };
    let mut render = |grid: bool| {
        renderer
            .render_prepared_world(
                &synthetic_art(),
                &terrain,
                &surfaces,
                &[],
                &[],
                camera,
                0,
                grid,
            )
            .unwrap()
    };
    render(false);
    let off = pixel();
    render(true);
    assert_ne!(pixel(), off);
    render(false);
    assert_eq!(pixel(), off);
}

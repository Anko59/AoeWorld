use super::*;
use aoe_core::{Seed, TileRect, WorldConfig};
use wasm_bindgen::JsCast;

fn camera() -> SceneCamera {
    SceneCamera {
        center: [35_000.0; 2],
        zoom: 1.0,
        viewport: [128.0; 2],
        focus_elevation_meters: 800.0,
    }
}

fn bounds(camera: SceneCamera) -> TileRect {
    let projection = Camera {
        center: camera.center,
        zoom: camera.zoom,
        viewport: camera.viewport,
        focus_elevation_meters: camera.focus_elevation_meters,
    };
    projection.visible_tiles_at_height(
        WorldConfig::new(70_000, 70_000, Seed(0)).unwrap(),
        1.0,
        camera.focus_elevation_meters,
    )
}

#[wasm_bindgen_test]
fn source_grid_geometry_is_focus_relative_and_bounded() {
    let camera = camera();
    let sprites = game_grid::grid_sprites(camera, bounds(camera));
    assert!(!sprites.is_empty());
    assert!(
        sprites
            .iter()
            .any(|sprite| sprite.position[0].abs() < 0.02 && sprite.position[1].abs() < 0.02)
    );
    let mut overview = camera;
    overview.zoom = 0.0001;
    let sprites = game_grid::grid_sprites(overview, TileRect::from_xywh(0, 0, 70_000, 70_000));
    assert!(!sprites.is_empty());
    assert!(sprites.len() <= 65_536);
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
        atlas: new_atlas(&canvas).unwrap(),
        source_atlas: Vec::new(),
        presentation: CanvasPresentation::new(128, 128),
    };
    let camera = camera();
    let terrain = [SceneTerrain {
        position: camera.center,
        material: 0,
        elevation_meters: 800.0,
        surface: SceneTerrainSurface::flat(800.0),
    }];
    let pixel = || {
        context
            .get_image_data(63.0, 63.0, 2.0, 2.0)
            .unwrap()
            .data()
            .0
    };
    let mut render = |grid: bool| {
        renderer
            .render_prepared_world(
                &synthetic_art(),
                &terrain,
                &[],
                &[],
                &[],
                camera,
                0,
                grid.then(|| bounds(camera)),
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

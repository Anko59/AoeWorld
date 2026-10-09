use super::*;
use crate::{GameArt, GameRenderer, SceneCamera, SceneTerrain, SceneTerrainSurface, game_grid};

#[wasm_bindgen_test]
async fn webgpu_live_source_grid_has_pixels_and_toggle_off_removes_them() {
    let document = web_sys::window().unwrap().document().unwrap();
    let canvas = document
        .create_element("canvas")
        .unwrap()
        .dyn_into::<HtmlCanvasElement>()
        .unwrap();
    canvas.set_width(128);
    canvas.set_height(128);
    let mut renderer = Renderer::new(canvas)
        .await
        .expect("software WebGPU renderer");
    let cleared = renderer
        .render_sprites(&[])
        .expect("clear-only presentation");
    assert!(
        cleared.did_present,
        "an empty submitted clear is still visible"
    );
    assert_eq!(cleared.draw_calls, 0);
    let mut game = GameRenderer::WebGpu(Box::new(renderer));
    game.upload_game_atlas(&vec![
        255;
        (3 * crate::GAME_ATLAS_SIDE * crate::GAME_ATLAS_SIDE * 4)
            as usize
    ])
    .unwrap();
    let camera = SceneCamera {
        center: [35_000.0; 2],
        zoom: 1.0,
        viewport: [128.0; 2],
        focus_elevation_meters: 800.0,
    };
    let terrain = [SceneTerrain {
        appearance: None,
        position: camera.center,
        material: 0,
        elevation_meters: 800.0,
        surface: SceneTerrainSurface::flat(800.0),
    }];
    let art = GameArt {
        walking: Vec::new(),
        standing: Vec::new(),
        grass: Vec::new(),
        terrain: std::array::from_fn(|_| Vec::new()),
        terrain_topology: [None; 7],
        terrain_world: None,
        resources: std::array::from_fn(|_| Vec::new()),
        tree_shadows: Vec::new(),
        tree_families: Default::default(),
    };
    let projection = aoe_core::Camera {
        center: camera.center,
        zoom: camera.zoom,
        viewport: camera.viewport,
        focus_elevation_meters: camera.focus_elevation_meters,
    };
    let config = aoe_core::WorldConfig::new(70_000, 70_000, aoe_core::Seed(0)).unwrap();
    let bounds = projection.visible_tiles_at_height(config, 1.0, 800.0);
    let count = game_grid::grid_sprites(camera, bounds).len();
    assert!(count > 0);
    let mut pixels = Vec::new();
    for enabled in [false, true, false] {
        game.render_prepared_world(
            &art,
            &terrain,
            &[],
            &[],
            &[],
            camera,
            0,
            enabled.then_some(bounds),
        )
        .unwrap();
        let GameRenderer::WebGpu(renderer) = &game else {
            unreachable!()
        };
        pixels.push(gpu_pixel(renderer, if enabled { count } else { 0 }).await);
    }
    assert_ne!(pixels[0], pixels[1]);
    assert_eq!(pixels[0], pixels[2]);
    let GameRenderer::WebGpu(renderer) = &game else {
        unreachable!()
    };
    renderer.device.destroy();
}

// Read production instance data with the real pipeline into a GPU attachment.
async fn gpu_pixel(renderer: &Renderer, count: usize) -> [u8; 4] {
    // Same production driver/pass and exact original [64,64] probe, with the
    // shared bounded map-error/unmap and format-swizzle ownership contract.
    super::terrain_blend::read_pixel(renderer, count as u32, [64, 64]).await
}

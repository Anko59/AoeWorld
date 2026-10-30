use super::*;
use wasm_bindgen::JsCast;

#[wasm_bindgen::prelude::wasm_bindgen(
    inline_js = "export function planar_turn(){return new Promise(resolve=>requestAnimationFrame(()=>setTimeout(resolve,0)));} export function planar_pixel(canvas,x,y){const gl=canvas.getContext('webgl2');gl.finish();const p=new Uint8Array(4);gl.readPixels(x,127-y,1,1,gl.RGBA,gl.UNSIGNED_BYTE,p);return p;}"
)]
extern "C" {
    fn planar_turn() -> js_sys::Promise;
    fn planar_pixel(canvas: &web_sys::HtmlCanvasElement, x: u32, y: u32) -> Vec<u8>;
}

#[wasm_bindgen_test]
async fn native_planar_texture_and_residency_directory_match_in_all_three_backends() {
    let mut art = fixture::art();
    art.grass.clear();
    art.terrain = Default::default();
    art.terrain_topology = [None; 12];
    let a = AtlasAddress::new(8, [2.0, 2.0, 1.0, 1.0].map(|n| n / 2048.0));
    let b = AtlasAddress::new(8, [4.0, 2.0, 1.0, 1.0].map(|n| n / 2048.0));
    art.terrain[0] = (0..7)
        .map(|i| crate::GameFrame {
            stream: 0,
            atlas: if i == 1 { b } else { a },
            size: [256.0; 2],
            anchor: [0.0; 2],
        })
        .collect();
    art.terrain_topology[0] = Some(TerrainTopology::Dds {
        size: [512, 256],
        period: [10, 10],
        level: 0,
    });
    let terrain: Vec<_> = (-8..9)
        .flat_map(|y| {
            (-8..9).map(move |x| crate::SceneTerrain {
                position: [x as f64 + 0.5, y as f64 + 0.5],
                material: 0,
                elevation_meters: 0.0,
                surface: crate::SceneTerrainSurface::flat(0.0),
                appearance: None,
            })
        })
        .collect();
    let camera = crate::SceneCamera {
        center: [5.0, 0.5],
        zoom: 1.0,
        viewport: [128.0; 2],
        focus_elevation_meters: 0.0,
    };
    let projection = aoe_core::Camera {
        center: camera.center,
        zoom: camera.zoom,
        viewport: camera.viewport,
        focus_elevation_meters: 0.0,
    };
    let mut colors = [[210, 30, 40, 255], [25, 190, 75, 255]];
    for backend in ["canvas", "webgl2", "webgpu"] {
        let canvas: web_sys::HtmlCanvasElement = web_sys::window()
            .unwrap()
            .document()
            .unwrap()
            .create_element("canvas")
            .unwrap()
            .dyn_into()
            .unwrap();
        canvas.set_width(128);
        canvas.set_height(128);
        web_sys::window()
            .unwrap()
            .document()
            .unwrap()
            .body()
            .unwrap()
            .append_child(&canvas)
            .unwrap();
        let (mut renderer, canvas) = crate::GameRenderer::new_with_backend(canvas, Some(backend))
            .await
            .unwrap();
        let mut upload = renderer.begin_game_atlas(10).unwrap();
        for layer in 0..10 {
            upload.page_buffer(layer).unwrap();
            renderer.write_game_atlas_page(&mut upload, layer).unwrap();
        }
        renderer.finish_game_atlas(upload).unwrap();
        let rows = terrain_directory_rows(&art).unwrap();
        renderer
            .update_game_atlas(0, [0, 0], [2048, 2], &rows)
            .unwrap();
        for turn in 0..2 {
            wasm_bindgen_futures::JsFuture::from(planar_turn())
                .await
                .unwrap();
            for (x, color) in [2, 4].into_iter().zip(colors) {
                renderer
                    .update_game_atlas(8, [x, 2], [1, 1], &color)
                    .unwrap();
            }
            assert!(
                renderer
                    .render_world(&art, &terrain, &[], &[], camera, 0, false)
                    .unwrap()
            );
            for [x, y] in [[44, 60], [84, 60], [44, 70], [84, 70]] {
                let world = projection.screen_to_world(aoe_core::ScreenPoint {
                    x: x as f64 + 0.5,
                    y: y as f64 + 0.5,
                });
                let expected = colors[usize::from(world[0].rem_euclid(10.0) >= 5.0)];
                let actual = read(&renderer, &canvas, [x, y]).await;
                assert_eq!(
                    actual, expected,
                    "{backend} turn {turn} at {x},{y} world {world:?}"
                );
            }
            colors.swap(0, 1);
        }
        let floor_colors = [
            [30, 70, 160, 255],
            [210, 160, 35, 255],
            [105, 35, 170, 255],
            [35, 170, 180, 255],
        ];
        let f = crate::GameFrame {
            stream: 0,
            atlas: AtlasAddress::new(8, [6.0, 2.0, 4.0, 1.0].map(|n| n / 2048.0)),
            size: [256.0; 2],
            anchor: [0.0; 2],
        };
        art.terrain[6] = vec![f; 5];
        art.terrain_topology[6] = Some(TerrainTopology::Dds {
            size: [256; 2],
            period: [10; 2],
            level: 0,
        });
        let floor_terrain: Vec<_> = terrain
            .iter()
            .map(|t| crate::SceneTerrain {
                appearance: Some(crate::SceneTerrainAppearance {
                    canopy_strength: 0,
                    floor_strength: 1000,
                    palette: 5,
                    exposure: 0,
                    height_band: 0,
                }),
                ..*t
            })
            .collect();
        let rows = terrain_directory_rows(&art).unwrap();
        renderer
            .update_game_atlas(0, [0, 0], [2048, 2], &rows)
            .unwrap();
        renderer
            .update_game_atlas(8, [6, 2], [4, 1], &floor_colors.concat())
            .unwrap();
        wasm_bindgen_futures::JsFuture::from(planar_turn())
            .await
            .unwrap();
        assert!(
            renderer
                .render_world(&art, &floor_terrain, &[], &[], camera, 0, false)
                .unwrap()
        );
        for [x, y] in [[44, 60], [84, 60], [44, 70], [84, 70]] {
            let world = projection.screen_to_world(aoe_core::ScreenPoint {
                x: x as f64 + 0.5,
                y: y as f64 + 0.5,
            });
            let index = (0.5 + world[0].rem_euclid(10.0) / 10.0 * 3.0).floor() as usize;
            assert_eq!(
                read(&renderer, &canvas, [x, y]).await,
                floor_colors[index],
                "independent forest-floor grid {backend} at {world:?}"
            );
        }
        art.terrain[6].clear();
        art.terrain_topology[6] = None;
        if let crate::GameRenderer::WebGpu(gpu) = &renderer {
            gpu.device.destroy();
        }
        canvas.remove();
    }
}

async fn read(
    renderer: &crate::GameRenderer,
    canvas: &web_sys::HtmlCanvasElement,
    [x, y]: [u32; 2],
) -> Vec<u8> {
    match renderer {
        crate::GameRenderer::Canvas { context, .. } => {
            context
                .get_image_data(x as f64, y as f64, 1.0, 1.0)
                .unwrap()
                .data()
                .0
        }
        crate::GameRenderer::WebGl(_) => planar_pixel(canvas, x, y),
        crate::GameRenderer::WebGpu(gpu) => {
            crate::web::tests::terrain_blend::read_pixel(gpu, gpu.staging.len() as u32, [x, y])
                .await
                .to_vec()
        }
    }
}

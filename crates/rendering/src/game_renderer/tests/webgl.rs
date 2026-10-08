//! Real WebGL2 program/driver pixel tests. Missing contexts are failures, not skips.
use super::*;
use crate::surface_mesh::{ProjectedSurfaceTriangle, SurfacePoint};
use crate::web::surface_instance;
use aoe_core::ScreenPoint;
use wasm_bindgen::prelude::*;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn webgl_v2_floor_pixels_match_canvas_kernel_and_preserve_three_page_abi() {
    let mut pixels = vec![0; crate::GAME_ATLAS_BYTES];
    for (page, texel) in [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]]
        .iter()
        .enumerate()
    {
        let start = page * crate::GAME_ATLAS_PAGE_BYTES;
        pixels[start..start + 4].copy_from_slice(texel);
    }
    let (canvas, mut renderer) = target(&pixels);
    let address = |page| crate::AtlasAddress {
        page,
        uv: [0.0, 0.0, 1.0 / 2048.0, 1.0 / 2048.0],
    };
    let mut face = triangle(
        [[16.0, 16.0], [112.0, 16.0], [16.0, 112.0]],
        [0.0; 3],
        [0.0; 3],
    );
    face.texture_uv = Some(address(0));
    face.texture_blend = Some([address(1), address(2)]);
    assert_eq!(instance(&face).pages[3], 0);
    let frame = |page| GameFrame {
        atlas: address(page),
        size: [1.0; 2],
        anchor: [0.0; 2],
    };
    let mut art = GameArt {
        walking: Vec::new(),
        standing: Vec::new(),
        grass: vec![frame(0)],
        terrain: std::array::from_fn(|_| vec![frame(0)]),
        terrain_topology: [None; 7],
        resources: std::array::from_fn(|_| Vec::new()),
        tree_shadows: Vec::new(),
    };
    art.terrain[2] = vec![frame(1)];
    art.terrain[6] = vec![frame(2)];
    for palette in 0..6 {
        face.appearance =
            crate::surface_mesh::landscape::pack(Some(crate::SceneTerrainAppearance {
                floor_strength: 650,
                canopy_strength: 650,
                palette,
                exposure: 1,
                height_band: 2,
            }));
        face.tint = 1;
        let packet = instance(&face);
        assert_eq!(packet.pages[..3], [0, 1, 2]);
        let expected = crate::surface_mesh::landscape::texel(
            [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]],
            crate::surface_mesh::landscape::floor_weights(packet.pages[3]),
            1,
            packet.pages[3],
        );
        render(&mut renderer, &mut [packet]);
        assert_pixel(&canvas, 48, 48, expected);
        assert_pixel(&canvas, 32, 32, expected);
        assert_ne!(expected, [82, 86, 86, 255]);
        for floor_strength in [0, 650] {
            let mut dirt = face;
            dirt.material = 2;
            dirt.appearance =
                crate::surface_mesh::landscape::pack(Some(crate::SceneTerrainAppearance {
                    floor_strength,
                    canopy_strength: 650,
                    palette,
                    exposure: 1,
                    height_band: 2,
                }));
            apply_terrain_textures(std::slice::from_mut(&mut dirt), &art);
            let packet = instance(&dirt);
            assert_eq!(dirt.texture_uv, Some(address(1)));
            assert_eq!(dirt.texture_blend, Some([address(2), address(1)]));
            assert_eq!(packet.pages[..3], [1, 2, 1]);
            let expected = crate::surface_mesh::landscape::texel(
                [[0, 255, 0, 255], [0, 0, 255, 255], [0, 255, 0, 255]],
                crate::surface_mesh::landscape::floor_weights(packet.pages[3]),
                1,
                packet.pages[3],
            );
            render(&mut renderer, &mut [packet]);
            assert_pixel(&canvas, 48, 48, expected);
            assert_pixel(&canvas, 32, 32, expected);
        }
    }
}

#[wasm_bindgen_test]
fn webgl_backing_axis_limit_and_zero_size_never_report_a_presented_frame() {
    let (canvas, mut renderer) = target(&atlas(&[]));
    assert!(renderer.resize(4097, 1).is_err());
    assert_eq!((canvas.width(), canvas.height()), (128, 128));
    assert!(renderer.resize(1, 4097).is_err());
    canvas.set_width(0);
    assert!(!renderer.render(&mut []).expect("zero-size context"));
}

#[wasm_bindgen_test]
fn canvas_zero_backing_size_does_not_advance_presentation() {
    let document = web_sys::window().unwrap().document().unwrap();
    let canvas = document
        .create_element("canvas")
        .unwrap()
        .dyn_into::<HtmlCanvasElement>()
        .unwrap();
    canvas.set_width(0);
    canvas.set_height(128);
    let mut renderer = GameRenderer::Canvas {
        context: context(&canvas).unwrap(),
        atlas: [None, None, None],
        presentation: CanvasPresentation::new(0, 128),
        source_atlas: Vec::new(),
        canvas,
    };
    let art = GameArt {
        walking: Vec::new(),
        standing: Vec::new(),
        grass: Vec::new(),
        terrain: std::array::from_fn(|_| Vec::new()),
        resources: std::array::from_fn(|_| Vec::new()),
        tree_shadows: Vec::new(),
        terrain_topology: [None; 7],
    };
    let camera = SceneCamera {
        center: [0.0; 2],
        zoom: 1.0,
        viewport: [0.0, 128.0],
        focus_elevation_meters: 0.0,
    };
    assert!(
        !renderer
            .render_prepared_world(&art, &[], &[], &[], &[], camera, 0, None)
            .unwrap()
    );
}

#[wasm_bindgen_test]
fn gpu_depth_normalization_preserves_original_iterator_bits() {
    let sprite = Sprite {
        position: [0.0; 2],
        radius: [0.0; 2],
        color: [1.0; 4],
        uv: [0.0; 4],
        depths: [0.0; 4],
        terrain_blend: [[0.0; 4]; 2],
        pages: [0; 4],
    };
    let special = [
        f32::NEG_INFINITY,
        f32::INFINITY,
        f32::NAN,
        -0.0,
        0.0,
        f32::EPSILON,
        f32::MIN_POSITIVE,
        -f32::MAX,
        f32::MAX,
    ];
    let mut seed = 0x1795_ab45_u32;
    for case in 0..1_024 {
        let mut actual = vec![sprite; 3];
        for item in &mut actual {
            for depth in &mut item.depths {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                *depth = if case < special.len() {
                    special[case]
                } else {
                    f32::from_bits(seed)
                };
            }
        }
        let mut expected = actual.clone();
        let (minimum, maximum) = expected
            .iter()
            .flat_map(|item| item.depths[..3].iter().copied())
            .filter(|depth| depth.is_finite())
            .fold(None, |range: Option<(f32, f32)>, depth| {
                Some(range.map_or((depth, depth), |(minimum, maximum)| {
                    (minimum.min(depth), maximum.max(depth))
                }))
            })
            .unwrap_or((0.0, 0.0));
        let span = maximum - minimum;
        for item in &mut expected {
            for depth in &mut item.depths {
                *depth = if *depth == f32::NEG_INFINITY {
                    1.0
                } else if !depth.is_finite() {
                    0.0
                } else if span <= f32::EPSILON {
                    0.5
                } else {
                    ((maximum - *depth) / span).clamp(0.0, 1.0)
                };
            }
        }
        crate::web::normalize_depths(&mut actual);
        for (actual, expected) in actual.iter().zip(&expected) {
            assert_eq!(
                actual.depths.map(f32::to_bits),
                expected.depths.map(f32::to_bits)
            );
        }
    }
}

#[wasm_bindgen(inline_js = "
export function webgl_test_pixel(canvas, x, y, output) {
    const gl = canvas.getContext('webgl2');
    if (!gl || gl.isContextLost()) throw new Error('WebGL2 test context unavailable');
    gl.readPixels(x, canvas.height - 1 - y, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, output);
    const code = gl.getError();
    if (code !== gl.NO_ERROR) throw new Error('WebGL2 test readPixels error ' + code);
}
")]
extern "C" {
    #[wasm_bindgen(catch)]
    fn webgl_test_pixel(
        canvas: &HtmlCanvasElement,
        x: u32,
        y: u32,
        output: &mut [u8],
    ) -> Result<(), JsValue>;
}

#[wasm_bindgen_test]
fn webgl_procedural_materials_match_shared_canvas_kernel() {
    let source = [170, 85, 40, 255];
    let (canvas, mut renderer) = target(&atlas(&[source]));
    for tint in (5..=10).chain([12]).chain(21..=26) {
        let mut face = triangle(
            [[16.0, 16.0], [112.0, 16.0], [16.0, 112.0]],
            [0.0; 3],
            [0.0; 3],
        );
        face.texture_uv = Some(crate::AtlasAddress {
            page: 0,
            uv: rect(0.0),
        });
        face.tint = tint;
        let mut packet = [surface_instance(&face, [128.0; 2], 0.0)];
        render(&mut renderer, &mut packet);
        assert_pixel(
            &canvas,
            48,
            48,
            crate::surface_mesh::procedural_tint(source, tint),
        );
    }
}

fn target(atlas: &[u8]) -> (HtmlCanvasElement, WebGlRenderer) {
    let canvas = web_sys::window()
        .expect("browser window")
        .document()
        .expect("browser document")
        .create_element("canvas")
        .expect("create canvas")
        .dyn_into::<HtmlCanvasElement>()
        .expect("canvas element");
    canvas.set_width(128);
    canvas.set_height(128);
    let mut renderer =
        WebGlRenderer::new(&canvas).expect("actual WebGL2 context and linked shaders");
    renderer
        .upload(atlas)
        .expect("actual WebGL2 native atlas upload");
    (canvas, renderer)
}

fn atlas(texels: &[[u8; 4]]) -> Vec<u8> {
    let mut result = vec![0; crate::GAME_ATLAS_BYTES];
    for (index, texel) in texels.iter().enumerate() {
        result[index * 4..index * 4 + 4].copy_from_slice(texel);
    }
    result
}

fn pixel(canvas: &HtmlCanvasElement, x: u32, y: u32) -> [u8; 4] {
    let mut result = [0; 4];
    // Synchronous read immediately after render: preserveDrawingBuffer is false.
    webgl_test_pixel(canvas, x, y, &mut result).expect("actual rendered WebGL2 pixel");
    result
}

fn assert_pixel(canvas: &HtmlCanvasElement, x: u32, y: u32, expected: [u8; 4]) {
    let actual = pixel(canvas, x, y);
    for (actual, expected) in actual.into_iter().zip(expected) {
        assert!(
            actual.abs_diff(expected) <= 1,
            "pixel {x},{y}: channel {actual}, expected {expected}"
        );
    }
}

fn render(renderer: &mut WebGlRenderer, sprites: &mut [Sprite]) {
    assert_eq!(std::mem::size_of::<Sprite>(), 112, "shared GPU sprite ABI");
    assert!(
        renderer
            .render(sprites)
            .expect("actual WebGL2 instanced render"),
        "frame not presented"
    );
}

fn rect(x: f32) -> [f32; 4] {
    [x / 2048.0, 0.0, 1.0 / 2048.0, 1.0 / 2048.0]
}

fn sprite(x: f32, depth: f32) -> Sprite {
    Sprite {
        position: [0.0; 2],
        radius: [0.25; 2],
        color: [1.0; 4],
        uv: rect(x),
        depths: [depth; 4],
        terrain_blend: [[0.0; 4]; 2],
        pages: [0; 4],
    }
}

fn triangle(
    screen: [[f64; 2]; 3],
    elevation: [f64; 3],
    color: [f32; 3],
) -> ProjectedSurfaceTriangle {
    ProjectedSurfaceTriangle {
        appearance: 0,
        points: std::array::from_fn(|index| SurfacePoint {
            world: [0.0, 0.0, elevation[index]],
            screen: ScreenPoint {
                x: screen[index][0],
                y: screen[index][1],
            },
        }),
        color,
        tile: [0, 0],
        skirt: false,
        material: 0,
        texture_mode: 4,
        tint: 0,
        texture_uv: None,
        texture_blend: None,
        texture_tile: [0; 2],
        texture_materials: None,
        pickable: true,
        order: 0,
    }
}

fn instance(triangle: &ProjectedSurfaceTriangle) -> Sprite {
    surface_instance(triangle, [128.0; 2], 0.0)
}

#[wasm_bindgen_test]
fn webgl_crossing_surface_depth_is_resolved_per_pixel_not_average_order() {
    let (canvas, mut renderer) = target(&atlas(&[]));
    let screen = [[20.0, 20.0], [108.0, 20.0], [64.0, 108.0]];
    let red = triangle(screen, [0.0, 0.0, 20.0], [1.0, 0.0, 0.0]);
    let blue = triangle(screen, [10.0; 3], [0.0, 0.0, 1.0]);
    // Test both submission orders: each triangle is closer at a different pixel.
    for triangles in [[red, blue], [blue, red]] {
        let mut sprites = triangles.map(|triangle| instance(&triangle));
        render(&mut renderer, &mut sprites);
        assert_pixel(&canvas, 64, 30, [0, 0, 255, 255]);
        assert_pixel(&canvas, 64, 90, [255, 0, 0, 255]);
    }
}

#[wasm_bindgen_test]
fn webgl_transparent_sprite_discards_without_writing_depth() {
    let (canvas, mut renderer) = target(&atlas(&[[255, 0, 0, 0], [0, 255, 0, 255]]));
    let ground = triangle(
        [[16.0, 16.0], [112.0, 16.0], [64.0, 112.0]],
        [0.0; 3],
        [1.0, 0.0, 0.0],
    );
    let mut sprites = [instance(&ground), sprite(0.0, 100.0), sprite(1.0, 1.0)];
    render(&mut renderer, &mut sprites);
    // The green sprite is behind the transparent sprite, but must remain visible.
    assert_pixel(&canvas, 64, 64, [0, 255, 0, 255]);
    assert_pixel(&canvas, 64, 30, [255, 0, 0, 255]);
}

#[wasm_bindgen_test]
fn webgl_equal_depth_ties_preserve_submission_order_and_straight_alpha() {
    let (canvas, mut renderer) = target(&atlas(&[[255, 0, 0, 255], [0, 255, 0, 128]]));
    let mut sprites = [sprite(0.0, 8.0), sprite(1.0, 8.0)];
    render(&mut renderer, &mut sprites);
    assert_pixel(&canvas, 64, 64, [127, 128, 0, 255]);
    let mut reversed = [sprite(1.0, 8.0), sprite(0.0, 8.0)];
    render(&mut renderer, &mut reversed);
    assert_pixel(&canvas, 64, 64, [255, 0, 0, 255]);
}

#[wasm_bindgen_test]
fn webgl_native_terrain_water_tint_preserves_texel_center_and_alpha() {
    let (canvas, mut renderer) = target(&atlas(&[[100, 100, 100, 255]]));
    let mut water = triangle(
        [[20.0, 20.0], [108.0, 20.0], [64.0, 108.0]],
        [0.0; 3],
        [0.0; 3],
    );
    water.texture_uv = Some(crate::AtlasAddress {
        page: 0,
        uv: rect(0.0),
    });
    water.tint = 4;
    let mut sprites = [instance(&water)];
    render(&mut renderer, &mut sprites);
    assert_pixel(&canvas, 64, 64, [91, 102, 113, 255]);
}

#[wasm_bindgen_test]
fn webgl_three_native_materials_use_shared_barycentric_weights() {
    let (canvas, mut renderer) = target(&atlas(&[
        [255, 0, 0, 255],
        [0, 255, 0, 255],
        [0, 0, 255, 255],
    ]));
    let mut blended = triangle(
        [[16.0, 16.0], [112.0, 16.0], [16.0, 112.0]],
        [0.0; 3],
        [0.0; 3],
    );
    blended.texture_uv = Some(crate::AtlasAddress {
        page: 0,
        uv: rect(0.0),
    });
    blended.texture_blend = Some([
        crate::AtlasAddress {
            page: 0,
            uv: rect(1.0),
        },
        crate::AtlasAddress {
            page: 0,
            uv: rect(2.0),
        },
    ]);
    let mut sprites = [instance(&blended)];
    render(&mut renderer, &mut sprites);
    assert_pixel(&canvas, 48, 48, [82, 86, 86, 255]);
    assert_pixel(&canvas, 24, 24, [210, 23, 23, 255]);
}

#[wasm_bindgen_test]
fn webgl_negative_uv_width_mirrors_sprite_texels_without_row_flipping() {
    let (canvas, mut renderer) = target(&atlas(&[[255, 0, 0, 255], [0, 0, 255, 255]]));
    let mut flipped = sprite(0.0, 1.0);
    flipped.uv = [2.0 / 2048.0, 0.0, -2.0 / 2048.0, 1.0 / 2048.0];
    let mut sprites = [flipped];
    render(&mut renderer, &mut sprites);
    assert_pixel(&canvas, 54, 64, [0, 0, 255, 255]);
    assert_pixel(&canvas, 74, 64, [255, 0, 0, 255]);
}

#[wasm_bindgen_test]
fn webgl_solid_surface_mode_does_not_sample_empty_atlas_or_degenerate_quad() {
    let (canvas, mut renderer) = target(&atlas(&[]));
    let solid = triangle(
        [[16.0, 16.0], [112.0, 16.0], [16.0, 112.0]],
        [3.0; 3],
        [0.25, 0.5, 0.75],
    );
    let mut sprites = [instance(&solid)];
    render(&mut renderer, &mut sprites);
    assert_pixel(&canvas, 32, 32, [64, 128, 191, 255]);
    assert_pixel(&canvas, 110, 110, [41, 74, 36, 255]);
}

#[path = "webgl_pages.rs"]
mod pages;

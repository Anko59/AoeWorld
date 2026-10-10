use super::*;

fn page_atlas() -> Vec<u8> {
    let mut pixels = atlas(&[]);
    for (page, color) in [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]]
        .iter()
        .enumerate()
    {
        let start = page * crate::GAME_ATLAS_PAGE_BYTES;
        pixels[start..start + 4].copy_from_slice(color);
    }
    pixels
}

#[wasm_bindgen_test]
fn equal_uv_pages_keep_depth_ties_alpha_and_cross_page_blend() {
    let (canvas, mut renderer) = target(&page_atlas());
    for page in 0..3 {
        let mut body = sprite(0.0, 1.0);
        body.pages[0] = page;
        render(&mut renderer, &mut [body]);
        assert_pixel(
            &canvas,
            64,
            64,
            [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]][page as usize],
        );
    }
    let mut bodies = [sprite(0.0, 8.0); 3];
    bodies[1].pages[0] = 1;
    bodies[2].pages[0] = 2;
    render(&mut renderer, &mut bodies);
    assert_pixel(&canvas, 64, 64, [0, 0, 255, 255]);
    bodies.reverse();
    render(&mut renderer, &mut bodies);
    assert_pixel(&canvas, 64, 64, [255, 0, 0, 255]);
    let mut face = triangle(
        [[16.0, 16.0], [112.0, 16.0], [16.0, 112.0]],
        [0.0; 3],
        [0.0; 3],
    );
    face.texture_uv = Some(crate::AtlasAddress {
        page: 0,
        uv: rect(0.0),
    });
    face.texture_blend = Some([
        crate::AtlasAddress {
            page: 1,
            uv: rect(0.0),
        },
        crate::AtlasAddress {
            page: 2,
            uv: rect(0.0),
        },
    ]);
    render(&mut renderer, &mut [instance(&face)]);
    assert_pixel(&canvas, 48, 48, [82, 86, 86, 255]);
}

#[wasm_bindgen_test]
fn page_two_mirror_and_alpha_shadow_keep_source_page() {
    let mut pixels = page_atlas();
    let start = 2 * crate::GAME_ATLAS_PAGE_BYTES;
    pixels[start + 4..start + 8].copy_from_slice(&[255, 0, 0, 255]);
    let (canvas, mut renderer) = target(&pixels);
    let mut body = sprite(0.0, 1.0);
    body.pages[0] = 2;
    body.uv = [2.0 / 2048.0, 0.0, -2.0 / 2048.0, 1.0 / 2048.0];
    render(&mut renderer, &mut [body]);
    assert_pixel(&canvas, 54, 64, [255, 0, 0, 255]);
    assert_pixel(&canvas, 74, 64, [0, 0, 255, 255]);
    body.color = [0.0, 0.0, 0.0, 0.5];
    render(&mut renderer, &mut [body]);
    assert_pixel(&canvas, 64, 64, [20, 37, 18, 255]);
}

#[wasm_bindgen(inline_js = "
export function restrict_array_limits(canvas, layers) {
    const gl = canvas.getContext('webgl2');
    const original = gl.getParameter.bind(gl);
    gl.getParameter = name => name === (layers ? gl.MAX_ARRAY_TEXTURE_LAYERS : gl.MAX_VERTEX_ATTRIBS)
        ? (layers ? 2 : 6) : original(name);
}
export function restore_owned_pages(bridge, fail) {
    const source = bridge.atlasPixels;
    if (source.byteLength !== 50331648) throw new Error('restoration owner size');
    if (fail) bridge.initialize = () => { throw new Error('injected restoration error'); };
    bridge.onRestored();
    if (bridge.atlasPixels !== source) throw new Error('restoration cloned source');
}
")]
extern "C" {
    #[wasm_bindgen(catch)]
    fn restrict_array_limits(canvas: &HtmlCanvasElement, layers: bool) -> Result<(), JsValue>;
    #[wasm_bindgen(catch)]
    fn restore_owned_pages(bridge: &JsValue, fail: bool) -> Result<(), JsValue>;
}

#[wasm_bindgen_test]
fn insufficient_array_layers_or_integer_attribute_capacity_fail_initialization() {
    for layers in [false, true] {
        let (canvas, renderer) = target(&page_atlas());
        drop(renderer);
        restrict_array_limits(&canvas, layers).unwrap();
        assert!(WebGlRenderer::new(&canvas).is_err());
    }
}

#[wasm_bindgen_test]
fn restoration_reuploads_existing_owner_and_errors_never_present() {
    let (canvas, mut renderer) = target(&page_atlas());
    restore_owned_pages(renderer.test_bridge(), false).unwrap();
    let mut body = sprite(0.0, 1.0);
    body.pages[0] = 2;
    render(&mut renderer, &mut [body]);
    assert_pixel(&canvas, 64, 64, [0, 0, 255, 255]);
    body.pages[0] = 1;
    render(&mut renderer, &mut [body]);
    assert_pixel(&canvas, 64, 64, [0, 255, 0, 255]);
    restore_owned_pages(renderer.test_bridge(), true).unwrap();
    assert!(renderer.render(&mut [body]).is_err());
}

#[wasm_bindgen_test]
fn webgl_landscape_parity_fixture_matches_shared_expected_texels() {
    use crate::surface_mesh::landscape::parity;
    let (canvas, mut renderer) = target(&parity::atlas());
    let screen = [[16.0, 16.0], [112.0, 16.0], [16.0, 112.0]];
    for case in parity::cases() {
        let mut face = triangle(screen, [0.0; 3], [0.0; 3]);
        parity::apply(&case, &mut face, true);
        render(&mut renderer, &mut [instance(&face)]);
        let legacy = [48, 32].map(|at| pixel(&canvas, at, at));
        parity::apply(&case, &mut face, false);
        render(&mut renderer, &mut [instance(&face)]);
        for (index, at) in [48, 32].into_iter().enumerate() {
            assert_pixel(&canvas, at, at, case.expected.unwrap_or(legacy[index]));
        }
    }
}

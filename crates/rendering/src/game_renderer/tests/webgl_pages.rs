use super::*;

fn page_atlas() -> Vec<u8> {
    let mut pixels = atlas(&[]);
    for (page, color) in [
        [255, 0, 0, 255],
        [0, 255, 0, 255],
        [0, 0, 255, 255],
        [255, 0, 255, 255],
    ]
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
    for page in 0..4 {
        let mut body = sprite(0.0, 1.0);
        body.pages[0] = page;
        render(&mut renderer, &mut [body]);
        assert_pixel(
            &canvas,
            64,
            64,
            [
                [255, 0, 0, 255],
                [0, 255, 0, 255],
                [0, 0, 255, 255],
                [255, 0, 255, 255],
            ][page as usize],
        );
    }
    let mut bodies = [sprite(0.0, 8.0); 4];
    bodies[1].pages[0] = 1;
    bodies[2].pages[0] = 2;
    bodies[3].pages[0] = 3;
    render(&mut renderer, &mut bodies);
    assert_pixel(&canvas, 64, 64, [255, 0, 255, 255]);
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
    // Five colour pages plus three owner-mask pages are retained on restore.
    if (source.byteLength !== 134217728) throw new Error('restoration owner size');
    if (fail) bridge.initialize = () => { throw new Error('injected restoration error'); };
    bridge.onRestored();
    if (bridge.atlasPixels !== source) throw new Error('restoration cloned source');
}
export function restore_shared_atlas_after_memory_growth(bridge, memory) {
    const source = bridge.atlasPixels;
    if (source.buffer !== memory.buffer) throw new Error('atlas duplicated outside WASM');
    const offset = source.byteOffset;
    memory.grow(1);
    bridge.onRestored();
    if (bridge.atlasPixels.buffer !== memory.buffer || bridge.atlasPixels.byteOffset !== offset)
        throw new Error('restoration must refresh the retained WASM view without copying');
}
")]
extern "C" {
    #[wasm_bindgen(catch)]
    fn restrict_array_limits(canvas: &HtmlCanvasElement, layers: bool) -> Result<(), JsValue>;
    #[wasm_bindgen(catch)]
    fn restore_owned_pages(bridge: &JsValue, fail: bool) -> Result<(), JsValue>;
    #[wasm_bindgen(catch)]
    fn restore_shared_atlas_after_memory_growth(
        bridge: &JsValue,
        memory: &JsValue,
    ) -> Result<(), JsValue>;
}

#[wasm_bindgen_test]
fn webgl_restoration_reuses_the_wasm_atlas_even_after_linear_memory_grows() {
    let (canvas, mut renderer) = target(&page_atlas());
    // An extra 128 MiB JS restoration copy breached the unchanged whole-browser
    // RSS ceiling. The renderer must own one source and survive detached views.
    restore_shared_atlas_after_memory_growth(renderer.test_bridge(), &wasm_bindgen::memory())
        .unwrap();
    let mut body = sprite(0.0, 1.0);
    body.pages[0] = 2;
    render(&mut renderer, &mut [body]);
    assert_pixel(&canvas, 64, 64, [0, 0, 255, 255]);
    let usage = GameRenderer::WebGl(renderer).memory_usage();
    assert_eq!(usage[2], ("wasm_atlas_bytes", crate::GAME_ATLAS_BYTES));
    assert_eq!(usage[3], ("js_atlas_bytes", 0));
}

#[wasm_bindgen_test]
fn insufficient_array_layers_or_integer_attribute_capacity_fail_initialization() {
    for layers in [false, true] {
        let canvas = canvas();
        restrict_array_limits(&canvas, layers).unwrap();
        assert!(WebGlRenderer::new(&canvas).is_err());
    }
}

#[wasm_bindgen_test]
fn dropping_webgl_renderer_retires_its_context_without_browser_gc() -> Result<(), JsValue> {
    let (canvas, renderer) = target(&page_atlas());
    // Hold the exact live context across Drop so GC cannot supply the retirement.
    let gl = webgl_test_context(&canvas).unwrap();
    assert!(!context_is_lost(&gl));
    drop(renderer);
    // Return the failure without duplicate Rust panic stacks in runner output.
    if !context_is_lost(&gl) {
        return Err(JsValue::from_str("Drop must retire the owned context"));
    }
    Ok(())
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

#[wasm_bindgen(inline_js = "
export function bound_atlas_transfers(canvas) {
    const gl = canvas.getContext('webgl2');
    const upload = gl.texSubImage3D.bind(gl), finish = gl.finish.bind(gl);
    let pending = false, completed = 0;
    gl.texSubImage3D = (...args) => {
        if (pending) throw new Error('Previous atlas page transfer is still queued');
        pending = true;
        return upload(...args);
    };
    gl.finish = () => { finish(); if (pending) completed++; pending = false; };
    return () => completed;
}")]
extern "C" {
    fn bound_atlas_transfers(canvas: &HtmlCanvasElement) -> js_sys::Function;
}

#[wasm_bindgen_test]
fn webgl_atlas_upload_bounds_driver_transfer_staging_to_one_page() {
    let canvas = canvas();
    let mut renderer = WebGlRenderer::new(&canvas).unwrap();
    let completed = bound_atlas_transfers(&canvas);
    renderer.upload(&page_atlas()).unwrap();
    assert_eq!(completed.call0(&JsValue::NULL).unwrap().as_f64(), Some(8.0));
    let mut body = sprite(0.0, 1.0);
    body.pages[0] = 2;
    render(&mut renderer, &mut [body]);
    assert_pixel(&canvas, 64, 64, [0, 0, 255, 255]);
}

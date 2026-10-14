//! Real WebGL2 minification pixels: the same fixture and expectations as the
//! Canvas kernel, within the one-channel tolerance of the WebGL tests.
use super::fixture::{self, Placed};
use super::*;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(inline_js = "
export function webgl_minification_pixel(canvas, x, y, output) {
    const gl = canvas.getContext('webgl2');
    if (!gl || gl.isContextLost()) throw new Error('WebGL2 test context unavailable');
    gl.readPixels(x, canvas.height - 1 - y, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, output);
    const code = gl.getError();
    if (code !== gl.NO_ERROR) throw new Error('WebGL2 test readPixels error ' + code);
}
")]
extern "C" {
    #[wasm_bindgen(catch)]
    fn webgl_minification_pixel(
        canvas: &HtmlCanvasElement,
        x: u32,
        y: u32,
        output: &mut [u8],
    ) -> Result<(), JsValue>;
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
    canvas.set_width(fixture::VIEWPORT[0] as u32);
    canvas.set_height(fixture::VIEWPORT[1] as u32);
    let mut renderer =
        WebGlRenderer::new(&canvas).expect("actual WebGL2 context and linked shaders");
    renderer
        .upload(atlas)
        .expect("actual WebGL2 native atlas upload");
    (canvas, renderer)
}

fn render(placed: &Placed) -> HtmlCanvasElement {
    let (canvas, mut renderer) = target(&fixture::atlas());
    let mut sprites = [placed.sprite];
    assert_eq!(std::mem::size_of::<Sprite>(), 112, "shared GPU sprite ABI");
    assert!(
        renderer.render(&mut sprites).expect("actual WebGL2 render"),
        "frame not presented"
    );
    canvas
}

fn pixel(canvas: &HtmlCanvasElement, x: u32, y: u32) -> [u8; 4] {
    let mut result = [0; 4];
    // Synchronous read immediately after render: preserveDrawingBuffer is false.
    webgl_minification_pixel(canvas, x, y, &mut result).expect("actual rendered WebGL2 pixel");
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

#[wasm_bindgen_test]
fn webgl_quarter_size_checker_averages_without_single_pixel_extremes() {
    let placed = fixture::placed(fixture::CHECKER, [32.0, 32.0], [4.0; 2], false);
    let expected = fixture::box_average([
        fixture::checker([0, 0]),
        fixture::checker([0, 2]),
        fixture::checker([2, 0]),
        fixture::checker([2, 2]),
    ])
    .unwrap();
    assert_eq!(expected, [0, 128, 0, 255]);
    let canvas = render(&placed);
    let [left, top] = placed.left_top.map(|value| value as u32);
    for y in top..top + 4 {
        for x in left..left + 4 {
            assert_pixel(&canvas, x, y, expected);
        }
    }
    // Confinement: no red from the adjacent atlas entry, and the pixel past
    // the quad edge stays background.
    for [x, y] in [[left + 4, top], [left + 4, top + 4], [left - 1, top + 4]] {
        assert_pixel(&canvas, x, y, fixture::BACKGROUND);
    }
}

#[wasm_bindgen_test]
fn webgl_minified_edge_takes_no_colour_from_hidden_or_adjacent_texels() {
    let placed = fixture::placed(fixture::HALO, [40.0, 32.0], [8.0; 2], false);
    let canvas = render(&placed);
    let left = placed.left_top[0] as u32;
    let top = placed.left_top[1] as u32;
    let mixed = fixture::box_average([
        fixture::halo(7),
        fixture::halo(7),
        fixture::halo(9),
        fixture::halo(9),
    ])
    .unwrap();
    for row in 0..8 {
        for column in 0..8 {
            let expected = match column {
                0..=3 => fixture::GREEN,
                4 => fixture::over(mixed, fixture::BACKGROUND),
                _ => fixture::BACKGROUND,
            };
            assert_pixel(&canvas, left + column, top + row, expected);
        }
    }
    for row in 0..8 {
        assert_pixel(&canvas, left + 8, top + row, fixture::BACKGROUND);
    }
}

#[wasm_bindgen_test]
fn webgl_native_size_checker_stays_pixel_exact() {
    let placed = fixture::placed(fixture::CHECKER, [32.0, 32.0], [16.0; 2], false);
    let canvas = render(&placed);
    let left = placed.left_top[0] as u32;
    let top = placed.left_top[1] as u32;
    for y in 0..16_u32 {
        for x in 0..16_u32 {
            assert_pixel(&canvas, left + x, top + y, fixture::checker([x, y]));
        }
    }
    assert_pixel(&canvas, left + 16, top, fixture::BACKGROUND);
}

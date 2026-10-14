//! Canvas 2D kernel: exact minified pixels, exact native pixels, and
//! confinement to the sprite's own atlas rectangle.
use super::fixture::{self, Placed};
use super::*;

fn target() -> Option<(HtmlCanvasElement, CanvasRenderingContext2d)> {
    let canvas = web_sys::window()?
        .document()?
        .create_element("canvas")
        .ok()?
        .dyn_into::<HtmlCanvasElement>()
        .ok()?;
    canvas.set_width(fixture::VIEWPORT[0] as u32);
    canvas.set_height(fixture::VIEWPORT[1] as u32);
    let context = context(&canvas).ok()?;
    Some((canvas, context))
}

fn camera() -> SceneCamera {
    SceneCamera {
        center: [0.5; 2],
        zoom: 1.0,
        viewport: fixture::VIEWPORT,
        focus_elevation_meters: 0.0,
    }
}

fn rendered(placed: &Placed) -> Vec<u8> {
    let (canvas, canvas_context) = target().expect("actual Canvas target");
    let mut presentation =
        CanvasPresentation::new(fixture::VIEWPORT[0] as u32, fixture::VIEWPORT[1] as u32);
    let layers = [WorldLayer::Sprite(placed.sprite, placed.frame, 0.0, 1)];
    render_canvas_world(
        &canvas,
        &canvas_context,
        &fixture::atlas(),
        &mut presentation,
        &layers,
        camera(),
        false,
    )
    .expect("Canvas frame should render");
    presentation.color_buffer.clone()
}

#[wasm_bindgen_test]
fn canvas_quarter_size_checker_averages_without_single_pixel_extremes() {
    let placed = fixture::placed(
        fixture::CHECKER,
        [32.0, 32.0],
        [4.0; 2],
        false,
        fixture::PHASE,
    );
    // Every output pixel's four clamped taps land on the same checker parity
    // classes: two green and two black texels.
    let expected = fixture::box_average([
        fixture::checker([0, 0]),
        fixture::checker([0, 2]),
        fixture::checker([2, 0]),
        fixture::checker([2, 2]),
    ])
    .unwrap();
    assert_eq!(expected, [0, 128, 0, 255]);
    let image = rendered(&placed);
    let [left, top] = placed.left_top.map(|value| value as u32);
    let mut values = Vec::new();
    for y in top..top + 4 {
        for x in left..left + 4 {
            let pixel = fixture::pixel(&image, x, y);
            values.push(pixel);
            assert_eq!(pixel, expected, "minified pixel {x},{y}");
        }
    }
    // Bounded local contrast: no isolated single-pixel extremes remain.
    assert_eq!(values.iter().max().unwrap(), values.iter().min().unwrap());
    // The quad covers only its own rectangle: the floor/ceil bounds pixel
    // beyond the right/bottom edge stays background instead of sampling the
    // red neighbour entry.
    for [x, y] in [
        [left - 1, top],
        [left + 4, top],
        [left + 4, top + 4],
        [left, top + 4],
    ] {
        assert_eq!(
            fixture::pixel(&image, x, y),
            fixture::BACKGROUND,
            "outside pixel {x},{y}"
        );
    }
}

#[wasm_bindgen_test]
fn canvas_minified_edge_takes_no_colour_from_hidden_or_adjacent_texels() {
    let placed = fixture::placed(
        fixture::HALO,
        [40.0, 32.0],
        [8.0; 2],
        false,
        fixture::HALO_PHASE,
    );
    let image = rendered(&placed);
    let left = placed.left_top[0] as u32;
    let top = placed.left_top[1] as u32;
    // Column tap pairs (frame-local x), two texels per pixel, phase 3/8:
    // {0,0} clamped, {1,2}, {3,4}, {5,6} opaque green, {7,8} mixed, {9,10},
    // {11,12}, {13,14} fully transparent.
    let mixed = fixture::box_average([
        fixture::halo(7),
        fixture::halo(7),
        fixture::halo(8),
        fixture::halo(8),
    ])
    .unwrap();
    for row in 0..8 {
        for column in 0..8 {
            let expected = match column {
                0..=3 => fixture::GREEN,
                4 => fixture::over(mixed, fixture::BACKGROUND),
                _ => fixture::BACKGROUND,
            };
            assert_eq!(
                fixture::pixel(&image, left + column, top + row),
                expected,
                "halo pixel {column},{row}"
            );
        }
    }
    // The hidden red rgb never surfaces and the blue neighbour entry is
    // never sampled, including the bounds pixel past the quad edge.
    for row in 0..8 {
        assert_eq!(
            fixture::pixel(&image, left + 8, top + row),
            fixture::BACKGROUND
        );
    }
}

#[wasm_bindgen_test]
fn canvas_native_size_checker_stays_pixel_exact() {
    let placed = fixture::placed(
        fixture::CHECKER,
        [32.0, 32.0],
        [16.0; 2],
        false,
        fixture::PHASE,
    );
    let image = rendered(&placed);
    let left = placed.left_top[0] as u32;
    let top = placed.left_top[1] as u32;
    for y in 0..16_u32 {
        for x in 0..16_u32 {
            assert_eq!(
                fixture::pixel(&image, left + x, top + y),
                fixture::checker([x, y]),
                "native pixel {x},{y}"
            );
        }
    }
    // Magnification keeps the nearest kernel: the bounds pixel past the quad
    // edge is not drawn at all.
    assert_eq!(fixture::pixel(&image, left + 16, top), fixture::BACKGROUND);
}

#[wasm_bindgen_test]
fn canvas_flipped_minified_sprite_mirrors_within_its_own_rectangle() {
    let placed = fixture::placed(
        fixture::HALO,
        [40.0, 32.0],
        [8.0; 2],
        true,
        fixture::HALO_PHASE,
    );
    let image = rendered(&placed);
    let left = placed.left_top[0] as u32;
    let top = placed.left_top[1] as u32;
    // Mirrored span: screen-left columns tap the transparent half ({15,15}
    // clamped, {13,14}, {11,12}, {9,10}), column 4 straddles local {7,8},
    // and columns 5..7 tap {5,6}, {3,4}, {1,2} opaque green.
    let mixed = fixture::box_average([
        fixture::halo(7),
        fixture::halo(7),
        fixture::halo(8),
        fixture::halo(8),
    ])
    .unwrap();
    for row in 0..8 {
        for column in 0..8 {
            let expected = match column {
                0..=3 => fixture::BACKGROUND,
                4 => fixture::over(mixed, fixture::BACKGROUND),
                _ => fixture::GREEN,
            };
            assert_eq!(
                fixture::pixel(&image, left + column, top + row),
                expected,
                "flipped halo pixel {column},{row}"
            );
        }
    }
}

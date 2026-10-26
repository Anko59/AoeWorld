//! The fallback must render the same coverage effects as WebGPU, using real GL pixels.
use super::*;

fn effect_atlas() -> Vec<u8> {
    let mut pixels = vec![0; crate::GAME_ATLAS_BYTES];
    for y in 0..16 {
        for x in 0..16 {
            if (2..14).contains(&x) && (2..14).contains(&y) {
                pixels[(y * 2048 + x) * 4..(y * 2048 + x) * 4 + 4]
                    .copy_from_slice(&[220, 30, 80, 255]);
            }
            if !(2..4).contains(&x) || !(7..9).contains(&y) {
                pixels[(y * 2048 + x + 32) * 4..(y * 2048 + x + 32) * 4 + 4]
                    .copy_from_slice(&[240, 200, 20, 255]);
            }
        }
    }
    pixels
}
fn body(tag: u32) -> Sprite {
    Sprite {
        radius: [0.5; 2],
        uv: [0.0, 0.0, 16.0 / 2048.0, 16.0 / 2048.0],
        pages: [0, 0, 0, tag],
        ..sprite(0.0, 0.0)
    }
}
fn blocker(tag: u32) -> Sprite {
    Sprite {
        position: [0.5, 0.0],
        radius: [0.5, 0.75],
        uv: [32.0 / 2048.0, 0.0, 16.0 / 2048.0, 16.0 / 2048.0],
        depths: [1.0; 4],
        ..body(tag)
    }
}

#[wasm_bindgen_test]
fn webgl_unoccluded_units_do_not_allocate_a_coverage_backing() {
    let (canvas, mut renderer) = target(&effect_atlas());
    let actor = body(0).with_unit_contour(Some([0, 0, 255]));
    let size = |renderer: &WebGlRenderer| {
        js_sys::Array::from(
            &js_sys::Reflect::get(renderer.test_bridge(), &"coverageSize".into()).unwrap(),
        )
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect::<Vec<_>>()
    };
    // A unit contour can only appear behind an occluder. Allocating and
    // blitting a full viewport for an unobstructed scout wastes GPU memory
    // and stalls software-rendered browser captures without changing pixels.
    render(&mut renderer, &mut [actor]);
    assert_eq!(size(&renderer), [0.0, 0.0]);
    assert_pixel(&canvas, 48, 64, [220, 30, 80, 255]);
    render(&mut renderer, &mut [actor, blocker(4)]);
    assert_eq!(size(&renderer), [128.0, 128.0]);
    assert_pixel(&canvas, 87, 64, [0, 0, 255, 255]);
}

#[wasm_bindgen_test]
fn webgl_hidden_contours_preserve_holes_mirroring_player_colour_and_painter_ties() {
    let (canvas, mut renderer) = target(&effect_atlas());
    for colour in [[0, 0, 255], [0, 255, 0]] {
        for mirror in [false, true] {
            let mut actor = body(0).with_unit_contour(Some(colour));
            if mirror {
                actor.uv[0] += actor.uv[2];
                actor.uv[2] = -actor.uv[2];
            }
            for reverse in [false, true] {
                let mut scene = if reverse {
                    [blocker(4), actor]
                } else {
                    [actor, blocker(4)]
                };
                render(&mut renderer, &mut scene);
                assert_pixel(&canvas, 87, 64, [colour[0], colour[1], colour[2], 255]);
                assert_pixel(&canvas, 48, 64, [220, 30, 80, 255]);
                assert_pixel(&canvas, 72, 64, [220, 30, 80, 255]);
                assert_pixel(&canvas, 80, 64, [240, 200, 20, 255]);
                assert_pixel(&canvas, 95, 64, [240, 200, 20, 255]);
            }
        }
    }
    let mut actor = body(0).with_unit_contour(Some([0, 0, 255]));
    actor.depths = [1.0; 4];
    for (mut scene, expected) in [
        ([actor, blocker(4)], [0, 0, 255, 255]),
        ([blocker(4), actor], [220, 30, 80, 255]),
    ] {
        render(&mut renderer, &mut scene);
        assert_pixel(&canvas, 87, 64, expected);
    }
}

#[wasm_bindgen_test]
fn webgl_contours_require_tall_opaque_coverage_and_survive_resize() {
    let (canvas, mut renderer) = target(&effect_atlas());
    let actor = body(0).with_unit_contour(Some([0, 0, 255]));
    for tag in [0, 1, 8, 16] {
        render(&mut renderer, &mut [actor, blocker(tag)]);
        assert_pixel(&canvas, 87, 64, [240, 200, 20, 255]);
    }
    renderer.resize(256, 128).unwrap();
    render(&mut renderer, &mut [actor, blocker(4)]);
    assert_pixel(&canvas, 175, 64, [0, 0, 255, 255]);
    renderer.resize(128, 128).unwrap();
    let mut atlas = effect_atlas();
    for y in 0..16 {
        for x in 32..48 {
            let alpha = &mut atlas[(y * 2048 + x) * 4 + 3];
            if *alpha > 0 {
                *alpha = 128;
            }
        }
    }
    renderer.upload(&atlas).unwrap();
    render(&mut renderer, &mut [actor, blocker(4)]);
    assert_pixel(&canvas, 87, 64, [230, 115, 50, 255]);
}

#[wasm_bindgen_test]
fn webgl_resource_outline_and_glow_follow_visible_body_alpha() {
    let (canvas, mut renderer) = target(&effect_atlas());
    for mode in [0, 16, 32] {
        let resource = body(mode);
        render(&mut renderer, &mut [resource]);
        assert_pixel(&canvas, 64, 64, [220, 30, 80, 255]);
        assert_pixel(&canvas, 36, 64, [41, 74, 36, 255]);
        if mode == 0 {
            assert_pixel(&canvas, 40, 64, [220, 30, 80, 255]);
        } else {
            assert!(pixel(&canvas, 40, 64)[1] > 60, "warm rim missing");
        }
        if mode == 32 {
            assert!(pixel(&canvas, 44, 64)[1] > 50, "wide glow missing");
        } else {
            assert_pixel(&canvas, 44, 64, [220, 30, 80, 255]);
        }
        if mode != 0 {
            render(&mut renderer, &mut [resource, blocker(4)]);
            assert_pixel(&canvas, 87, 64, [240, 200, 20, 255]);
            assert_pixel(&canvas, 64, 64, [240, 200, 20, 255]);
            assert!(pixel(&canvas, 40, 64)[1] > 60);
        }
    }
}

#[wasm_bindgen_test]
fn owner_masks_recolour_only_marked_pixels_on_webgl() {
    let mut atlas = effect_atlas();
    for y in 0..16 {
        for x in 2..8 {
            let at = 3 * crate::GAME_ATLAS_PAGE_BYTES + (y * 2048 + x) * 4;
            atlas[at..at + 4].copy_from_slice(&[7, 255, 255, 0]);
        }
    }
    let (canvas, mut renderer) = target(&atlas);
    for colour in crate::PLAYER_COLOURS {
        let actor = body(0).with_owner(crate::SceneUnitVisual::new(false).with_owner(
            Some(colour),
            true,
            false,
        ));
        let mut scene = [actor];
        render(&mut renderer, &mut scene);
        assert_pixel(&canvas, 48, 64, [colour[0], colour[1], colour[2], 255]);
        assert_pixel(&canvas, 80, 64, [220, 30, 80, 255]);
    }
}

#[wasm_bindgen_test]
fn webgl_coverage_reuses_attachments_and_releases_idle_storage() {
    let (canvas, renderer) = target(&effect_atlas());
    let actor = body(0).with_unit_contour(Some([0, 0, 255]));
    let mut scene = GameRenderer::WebGl(renderer);
    assert_eq!(scene.memory_usage()[0].1, crate::GAME_ATLAS_BYTES);
    let GameRenderer::WebGl(ref mut renderer) = scene else {
        unreachable!()
    };
    let allocations = track_coverage_storage(renderer.test_bridge());
    render(renderer, &mut [actor, blocker(4)]);
    let retained = js_sys::Reflect::get(renderer.test_bridge(), &"mask".into()).unwrap();
    for _ in 0..4 {
        render(renderer, &mut [actor]);
        assert!(coverage_texture_is_live(renderer.test_bridge(), &retained));
        render(renderer, &mut [actor, blocker(4)]);
    }
    let commands = js_sys::Array::from(&allocations.call0(&JsValue::NULL).unwrap());
    assert_eq!(commands.get(0).as_f64(), Some(1.0));
    // Five effect frames submit only the actor, never its occluding tree.
    assert_eq!(commands.get(1).as_f64(), Some(5.0));
    for side in [112, 128, 112, 128] {
        renderer.resize(side, side).unwrap();
        render(renderer, &mut [actor, blocker(4)]);
        let current = js_sys::Reflect::get(renderer.test_bridge(), &"mask".into()).unwrap();
        assert!(js_sys::Object::is(&retained, &current));
        let size = js_sys::Array::from(
            &js_sys::Reflect::get(renderer.test_bridge(), &"coverageSize".into()).unwrap(),
        );
        assert_eq!(size.get(0).as_f64(), Some(f64::from(side)));
        assert_pixel(&canvas, side * 87 / 128, side / 2, [0, 0, 255, 255]);
    }
    // Coverage needs only R8UI + DEPTH_COMPONENT24 (four-byte storage).
    // World colour stays in the browser framebuffer, at full resolution.
    assert_eq!(
        scene.memory_usage()[0].1,
        crate::GAME_ATLAS_BYTES + 128 * 128 * 5
    );
    let GameRenderer::WebGl(ref mut renderer) = scene else {
        unreachable!()
    };
    renderer.resize(32, 32).unwrap();
    // Shrink immediately, even when no subsequent frame requests effects.
    assert_eq!(
        scene.memory_usage()[0].1,
        crate::GAME_ATLAS_BYTES + 32 * 32 * 5
    );
    let GameRenderer::WebGl(ref mut renderer) = scene else {
        unreachable!()
    };
    render(renderer, &mut [actor, blocker(4)]);
    assert_pixel(&canvas, 21, 16, [0, 0, 255, 255]);
    for _ in 0..59 {
        render(renderer, &mut [actor]);
        assert!(coverage_texture_is_live(renderer.test_bridge(), &retained));
    }
    render(renderer, &mut [actor]);
    assert!(!coverage_texture_is_live(renderer.test_bridge(), &retained));
    assert_eq!(scene.memory_usage()[0].1, crate::GAME_ATLAS_BYTES);
    let GameRenderer::WebGl(ref mut renderer) = scene else {
        unreachable!()
    };
    let size = js_sys::Array::from(
        &js_sys::Reflect::get(renderer.test_bridge(), &"coverageSize".into()).unwrap(),
    );
    assert_eq!(size.get(0).as_f64(), Some(0.0));
    assert_eq!(size.get(1).as_f64(), Some(0.0));
    render(renderer, &mut [actor, blocker(4)]);
    assert_pixel(&canvas, 21, 16, [0, 0, 255, 255]);
    renderer.resize(2048, 2048).unwrap();
    render(renderer, &mut [actor, blocker(4)]);
    // Large canvases keep full-resolution world colour and contour kernels;
    // only the shared coverage/depth pass uses half-size axes (one megapixel).
    assert_pixel(&canvas, 768, 1024, [220, 30, 80, 255]);
    assert_pixel(&canvas, 1407, 1024, [0, 0, 255, 255]);
    for mode in [16, 32] {
        render(renderer, &mut [body(mode), blocker(4)]);
        assert!(
            pixel(&canvas, 640, 1024)[1] > 60,
            "upsampled hover rim missing"
        );
        assert_pixel(&canvas, 768, 1024, [220, 30, 80, 255]);
        assert_pixel(&canvas, 1407, 1024, [240, 200, 20, 255]);
    }
    let bridge = renderer.test_bridge().clone();
    assert_eq!(
        scene.memory_usage()[0].1,
        crate::GAME_ATLAS_BYTES + 1024 * 1024 * 5
    );
    drop(scene);
    assert_eq!(
        js_sys::Reflect::get(&bridge, &"textureBytes".into())
            .unwrap()
            .as_f64(),
        Some(0.0)
    );
}

#[wasm_bindgen(inline_js = "
export function coverage_texture_is_live(bridge, texture) {
    return bridge.gl.isTexture(texture);
}
export function track_coverage_storage(bridge) {
    const gl = bridge.gl, allocate = gl.texImage2D.bind(gl);
    const program = gl.useProgram.bind(gl), draw = gl.drawArraysInstanced.bind(gl);
    const render = bridge.render.bind(bridge);
    let rendering = false;
    bridge.render = (...args) => {
        rendering = true;
        try { return render(...args); } finally { rendering = false; }
    };
    let allocations = 0, contours = 0, current;
    gl.texImage2D = (...args) => { allocations++; return allocate(...args); };
    gl.useProgram = value => { current = value; return program(value); };
    gl.drawArraysInstanced = (...args) => {
        if (current === bridge.programs[2]) contours += args[3];
        return draw(...args);
    };
    // The pixel oracle reads after render; only product submission must not.
    for (const name of ['finish', 'readPixels', 'getBufferSubData']) {
        const original = gl[name].bind(gl);
        gl[name] = (...args) => {
            if (rendering) throw new Error('Synchronous render operation: ' + name);
            return original(...args);
        };
    }
    return () => [allocations, contours];
}")]
extern "C" {
    fn coverage_texture_is_live(bridge: &JsValue, texture: &JsValue) -> bool;
    fn track_coverage_storage(bridge: &JsValue) -> js_sys::Function;
}

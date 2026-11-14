//! Observe real uploads and pixels, including invalidated GPU storage.
use super::*;

#[wasm_bindgen(inline_js = "
export function watch_gl_instance_uploads(bridge) {
    const gl = bridge.gl, original = gl.bufferSubData.bind(gl), calls = [];
    gl.bufferSubData = (target, offset, data, ...rest) => {
        if (target === gl.ARRAY_BUFFER) calls.push(offset, data.byteLength);
        return original(target, offset, data, ...rest);
    };
    return () => new Uint32Array(calls.splice(0));
}
")]
extern "C" {
    fn watch_gl_instance_uploads(bridge: &JsValue) -> js_sys::Function;
}

fn uploads(observer: &js_sys::Function) -> Vec<u32> {
    js_sys::Uint32Array::new(&observer.call0(&JsValue::UNDEFINED).unwrap()).to_vec()
}

fn bodies() -> [Sprite; 3] {
    [-0.6, 0.0, 0.6].map(|x| Sprite {
        position: [x, 0.0],
        radius: [0.2; 2],
        color: [1.0, 0.0, 0.0, 1.0],
        ..sprite(0.0, 1.0)
    })
}

fn present(renderer: &mut WebGlRenderer, source: &[Sprite]) {
    render(renderer, &mut source.to_vec());
}

#[wasm_bindgen_test]
async fn webgl_instance_uploads_follow_changes_skips_growth_and_restoration() {
    let (canvas, mut renderer) = target(&atlas(&[[255; 4]]));
    let observer = watch_gl_instance_uploads(renderer.test_bridge());
    let mut source = bodies();
    present(&mut renderer, &source);
    assert_eq!(uploads(&observer), [0, 336]);
    assert_pixel(&canvas, 64, 64, [255, 0, 0, 255]);

    present(&mut renderer, &source);
    assert!(
        uploads(&observer).is_empty(),
        "unchanged packets stay on the GPU"
    );
    source[1].color = [0.0, 0.0, 1.0, 1.0];
    present(&mut renderer, &source);
    assert_eq!(
        uploads(&observer),
        [112, 112],
        "only the middle packet changed"
    );
    assert_pixel(&canvas, 64, 64, [0, 0, 255, 255]);

    present(&mut renderer, &source[..2]);
    assert!(
        uploads(&observer).is_empty(),
        "shrinking retains an equal prefix"
    );
    present(&mut renderer, &source);
    assert_eq!(
        uploads(&observer),
        [224, 112],
        "growth restores the new tail"
    );
    let mut larger = source.to_vec();
    larger.push(Sprite {
        position: [2.0, 0.0],
        ..source[0]
    });
    present(&mut renderer, &larger);
    assert_eq!(
        uploads(&observer),
        [0, 448],
        "bufferData invalidates the prefix too"
    );

    // A skipped submission must not commit the attempted packet to the cache.
    canvas.set_width(0);
    larger[1].color = [1.0, 0.0, 0.0, 1.0];
    assert!(!renderer.render(&mut larger.clone()).unwrap());
    assert!(uploads(&observer).is_empty());
    renderer.resize(128, 128).unwrap();
    present(&mut renderer, &larger);
    assert_eq!(uploads(&observer), [112, 112]);
    assert_pixel(&canvas, 64, 64, [255, 0, 0, 255]);

    wasm_bindgen_futures::JsFuture::from(effects::lose_and_restore_context(renderer.test_bridge()))
        .await
        .unwrap();
    present(&mut renderer, &larger);
    assert_eq!(uploads(&observer), [0, 448], "restoration needs every byte");
    assert_pixel(&canvas, 64, 64, [255, 0, 0, 255]);
    present(&mut renderer, &larger);
    assert!(uploads(&observer).is_empty());

    // Identical NaN payloads are identical bytes; float equality is unsuitable.
    larger[1].terrain_blend[1][3] = f32::from_bits(0x7fc0_1234);
    present(&mut renderer, &larger);
    assert_eq!(uploads(&observer), [112, 112]);
    present(&mut renderer, &larger);
    assert!(uploads(&observer).is_empty());
    let mut scene = GameRenderer::WebGl(renderer);
    assert!(scene.frame_staging_bytes() >= larger.len() * std::mem::size_of::<Sprite>());
    scene.resize(0, 0);
}

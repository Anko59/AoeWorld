//! WebGPU must share the packet-retention contract of the compatibility GPU.
use super::terrain_blend::{assert_pixel, read_pixel, surface_renderer_with_atlas};
use super::*;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(inline_js = "
export function watch_gpu_instance_uploads() {
    const prototype = GPUQueue.prototype, original = prototype.writeBuffer, calls = [];
    prototype.writeBuffer = function(buffer, offset, data, start = 0, size) {
        const unit = data.BYTES_PER_ELEMENT ?? 1;
        calls.push(offset, size === undefined ? data.byteLength - start * unit : size * unit);
        return original.apply(this, arguments);
    };
    return finish => {
        if (finish) prototype.writeBuffer = original;
        return new Uint32Array(calls.splice(0));
    };
}
")]
extern "C" {
    fn watch_gpu_instance_uploads() -> js_sys::Function;
}

fn uploads(observer: &js_sys::Function, finish: bool) -> Vec<u32> {
    js_sys::Uint32Array::new(&observer.call1(&JsValue::UNDEFINED, &finish.into()).unwrap()).to_vec()
}

#[wasm_bindgen_test]
async fn webgpu_instance_uploads_retain_equal_packets_and_rewrite_reallocated_storage() {
    let mut atlas = vec![0; crate::GAME_ATLAS_BYTES];
    atlas[..4].copy_from_slice(&[255; 4]);
    let mut renderer = surface_renderer_with_atlas(&atlas).await;
    let observer = watch_gpu_instance_uploads();
    let mut source = [-0.6, 0.0, 0.6].map(|x| Sprite {
        position: [x, 0.0],
        radius: [0.2; 2],
        color: [1.0, 0.0, 0.0, 1.0],
        uv: [0.0, 0.0, 1.0 / 2048.0, 1.0 / 2048.0],
        ..solid_sprite()
    });
    assert!(renderer.render_sprites(&source).unwrap().did_present);
    assert_eq!(uploads(&observer, false), [0, 336]);
    assert_pixel(read_pixel(&renderer, 3, [64, 64]).await, [255, 0, 0, 255]);
    submitted_frame(&renderer).await;
    assert!(renderer.render_sprites(&source).unwrap().did_present);
    assert!(
        uploads(&observer, false).is_empty(),
        "identical bytes need no writeBuffer"
    );
    submitted_frame(&renderer).await;

    source[1].color = [0.0, 0.0, 1.0, 1.0];
    assert!(renderer.render_sprites(&source).unwrap().did_present);
    assert_eq!(uploads(&observer, false), [112, 112]);
    assert_pixel(read_pixel(&renderer, 3, [64, 64]).await, [0, 0, 255, 255]);
    submitted_frame(&renderer).await;
    assert!(renderer.render_sprites(&source[..2]).unwrap().did_present);
    assert!(uploads(&observer, false).is_empty());
    submitted_frame(&renderer).await;
    assert!(renderer.render_sprites(&source).unwrap().did_present);
    assert_eq!(uploads(&observer, false), [224, 112]);
    submitted_frame(&renderer).await;

    // Reallocation destroys the previous GPU packet even if its CPU bytes match.
    let next = renderer.instances.capacity() + 1;
    renderer
        .instances
        .ensure_capacity(next, &renderer.device)
        .unwrap();
    assert!(renderer.render_sprites(&source).unwrap().did_present);
    assert_eq!(uploads(&observer, false), [0, 336]);
    assert_pixel(read_pixel(&renderer, 3, [64, 64]).await, [0, 0, 255, 255]);
    assert!(renderer.staging_bytes() >= 2 * source.len() * std::mem::size_of::<Sprite>());
    assert!(uploads(&observer, true).is_empty());
    renderer.device.destroy();
}

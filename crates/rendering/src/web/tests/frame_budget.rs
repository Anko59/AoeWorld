use super::*;

#[wasm_bindgen_test]
async fn outstanding_frame_budget_skips_the_third_frame_then_resumes_after_completion() {
    let mut renderer = terrain_blend::surface_renderer().await;
    let first = renderer.render_sprites(&[solid_sprite()]).unwrap();
    let second = renderer.render_sprites(&[solid_sprite()]).unwrap();
    let skipped = renderer.render_sprites(&[solid_sprite()]).unwrap();
    assert!(first.did_present && second.did_present);
    assert!(!skipped.did_present);
    assert_eq!(skipped.draw_calls, 0);
    assert_eq!(skipped.gpu_buffer_bytes, first.gpu_buffer_bytes);
    submitted_frame(&renderer).await;
    let resumed = renderer.render_sprites(&[solid_sprite()]).unwrap();
    assert!(resumed.did_present);
    assert_eq!(resumed.draw_calls, first.draw_calls);
    renderer.device.destroy();
}

#[wasm_bindgen_test]
async fn contour_submission_excludes_ineligible_world_instances() {
    let mut renderer = terrain_blend::surface_renderer().await;
    let mut scene = vec![
        Sprite {
            pages: [0, 0, 0, Sprite::OCCLUDER],
            ..solid_sprite()
        };
        65
    ];
    scene[16].pages[3] = Sprite::UNIT_CONTOUR;
    scene[48].pages[3] = 16; // Hovered resource between ordinary trees.
    let capture = capture_contour_instances();
    let counters = renderer.render_sprites(&scene).unwrap();
    let count = capture.call0(&wasm_bindgen::JsValue::NULL).unwrap();
    assert_eq!(
        count.as_f64(),
        Some(2.0),
        "only eligible contour instances reach the GPU"
    );
    assert_eq!(
        counters.draw_calls, 3,
        "world plus two eligible contour runs"
    );
    submitted_frame(&renderer).await;
    renderer.device.destroy();
}

#[wasm_bindgen::prelude::wasm_bindgen(inline_js = "
export function capture_contour_instances() {
    const prototype = GPURenderPassEncoder.prototype, draw = prototype.draw;
    let count = 0;
    prototype.draw = function(...args) {
        if (this.label === 'hidden units and resource hover contours') count += args[1];
        return Reflect.apply(draw, this, args);
    };
    return () => { prototype.draw = draw; return count; };
}")]
extern "C" {
    fn capture_contour_instances() -> js_sys::Function;
}

#[wasm_bindgen_test]
async fn instance_fetch_uses_the_shared_112_byte_instanced_vertex_layout() {
    let capture = capture_instance_layout();
    let renderer = terrain_blend::surface_renderer().await;
    let valid = capture.call0(&wasm_bindgen::JsValue::NULL).unwrap();
    renderer.device.destroy();
    assert_eq!(
        valid.as_bool(),
        Some(true),
        "instance fetch must use vertex attributes, not repeated storage-buffer loads"
    );
}

#[wasm_bindgen::prelude::wasm_bindgen(inline_js = "
export function capture_instance_layout() {
    const prototype = GPUDevice.prototype, create = prototype.createRenderPipeline;
    let valid = false;
    prototype.createRenderPipeline = function(descriptor) {
        const buffer = descriptor.vertex.buffers?.[0];
        valid = buffer?.arrayStride === 112 && buffer.stepMode === 'instance'
            && buffer.attributes.length === 7
            && buffer.attributes[6].format === 'uint32x4';
        return Reflect.apply(create, this, [descriptor]);
    };
    return () => { prototype.createRenderPipeline = create; return valid; };
}")]
extern "C" {
    fn capture_instance_layout() -> js_sys::Function;
}

#[wasm_bindgen_test]
fn contour_runs_batch_adjacent_candidates_and_preserve_order_across_pages() {
    let mut scene = [solid_sprite(); 8];
    for (index, page, flags) in [(1, 4, 8), (2, 2, 16), (4, 3, 32), (6, 2, 8)] {
        scene[index].pages = [page, 0, 0, flags];
    }
    scene[0].color[3] = -2.0; // Surface appearance bits are not contour flags.
    scene[0].pages[3] = 56;
    scene[5].color[3] = f32::NAN;
    scene[5].pages[3] = 8;
    let mut ranges = vec![99..100];
    draw::write_contour_ranges(&scene, &mut ranges);
    assert_eq!(ranges, vec![1..3, 4..5, 6..7]);
    draw::write_contour_ranges(&[], &mut ranges);
    assert!(
        ranges.is_empty(),
        "a clear frame must not retain previous contour draws"
    );
}

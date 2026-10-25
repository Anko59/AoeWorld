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

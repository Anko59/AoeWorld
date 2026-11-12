use super::*;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn picking_borrows_the_presented_scene_until_replacement_or_reset() {
    let scene = Rc::new(());
    let other_view = Rc::new(());
    let mut cache = RenderFrameCache::default();
    assert!(cache.picking_units(&scene).is_none());
    assert!(cache.picking_resources(&scene).is_none());
    let old = unit([1.0, 2.0], false, false);
    cache.record_success(&scene, &art(), &[old], &[resource(9)], false, 0, camera());
    let units = cache.picking_units(&scene).unwrap();
    assert_eq!(units[0].position, old.position);
    assert!(std::ptr::eq(units, cache.picking_units(&scene).unwrap()));
    assert_eq!(
        cache
            .picking_resources(&scene)
            .unwrap()
            .map(|r| r.id)
            .collect::<Vec<_>>(),
        [9]
    );
    assert!(cache.picking_units(&other_view).is_none());
    assert!(cache.picking_resources(&other_view).is_none());
    // Decoding new art or preparing another frame does not replace the pixels.
    cache.invalidate();
    assert!(cache.needs_render(
        &scene,
        &art(),
        &[unit([2.0, 3.0], false, false)],
        &[resource(10)],
        false,
        0,
        camera()
    ));
    assert_eq!(
        cache.picking_units(&scene).unwrap()[0].position,
        old.position
    );
    assert_eq!(
        cache.picking_resources(&scene).unwrap().next().unwrap().id,
        9
    );
    cache.record_success(
        &scene,
        &art(),
        &[unit([2.0, 3.0], false, false)],
        &[resource(10)],
        false,
        0,
        camera(),
    );
    assert_eq!(cache.picking_units(&scene).unwrap()[0].position, [2.0, 3.0]);
    assert_eq!(
        cache.picking_resources(&scene).unwrap().next().unwrap().id,
        10
    );
    cache.clear();
    assert_eq!(
        Rc::strong_count(&scene),
        1,
        "reset releases the displayed scene"
    );
    assert!(cache.picking_units(&scene).is_none());
    assert!(cache.picking_resources(&scene).is_none());
}

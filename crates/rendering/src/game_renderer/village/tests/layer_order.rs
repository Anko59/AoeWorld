use super::*;
#[wasm_bindgen_test::wasm_bindgen_test]
fn composed_layer_sort_matches_two_stable_sorts_on_exact_depth_and_id_ties() {
    use bytemuck::Zeroable;
    let frame = GameFrame {
        atlas: crate::AtlasAddress {
            page: 0,
            uv: [0.0; 4],
        },
        size: [1.0; 2],
        anchor: [0.0; 2],
    };
    let camera = SceneCamera {
        center: [0.0; 2],
        zoom: 1.0,
        viewport: [128.0; 2],
        focus_elevation_meters: 0.0,
    };
    let objects = (0..96)
        .map(|index| {
            let mut sprite = Sprite::zeroed();
            sprite.color[0] = index as f32;
            let depth = [0.0, -0.0, 4.0, -1.0, f64::INFINITY, f64::NEG_INFINITY][index % 6];
            (sprite, frame, depth, (index % 4) as u64)
        })
        .collect::<Vec<_>>();
    // Original object pre-sort, followed by the old stable layer depth sort.
    let mut expected = objects.clone();
    expected.sort_by(|left, right| left.2.total_cmp(&right.2).then(left.3.cmp(&right.3)));
    expected.sort_by(|left, right| left.2.total_cmp(&right.2));
    let actual = ordered_world_layers([], objects, &[], camera);
    let actual = actual
        .iter()
        .map(|layer| {
            let WorldLayer::Sprite(sprite, _, _, _) = layer else {
                panic!("unexpected layer")
            };
            sprite.color[0]
        })
        .collect::<Vec<_>>();
    assert_eq!(
        actual,
        expected
            .iter()
            .map(|entry| entry.0.color[0])
            .collect::<Vec<_>>()
    );
}

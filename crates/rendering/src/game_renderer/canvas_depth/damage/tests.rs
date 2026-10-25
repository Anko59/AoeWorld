use super::*;
use crate::{GameFrame, web::Sprite};
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn unchanged_packed_nan_texture_words_are_not_animation_damage() {
    let address = crate::AtlasAddress {
        page: 0,
        uv: [f32::from_bits(0x7fc0_1234); 4],
    };
    let layer = WorldLayer::Surface(crate::ProjectedSurfaceTriangle {
        points: [[0.0, 0.0], [128.0, 0.0], [64.0, 128.0]].map(|[x, y]| {
            crate::surface_mesh::SurfacePoint {
                world: [x, y, 0.0],
                screen: aoe_core::ScreenPoint { x, y },
            }
        }),
        color: [0.4; 3],
        tile: [0; 2],
        skirt: false,
        material: 0,
        appearance: 0,
        floor_strengths: None,
        mix: [0; 6],
        texture_mode: 0,
        tint: 0,
        texture_uv: None,
        texture_blend: Some([address; 2]),
        texture_tile: [0; 2],
        texture_materials: None,
        pickable: true,
        order: 0,
    });
    let mut damage = Damage::default();
    damage.remember(&[layer], &[[1.0; 3]], [128.0; 2], [128; 2], false, &[]);
    assert!(
        damage
            .regions(&[layer], &[[1.0; 3]], [128.0; 2], [128; 2], false, &[])
            .is_empty()
    );
}

#[wasm_bindgen_test]
fn distant_animations_do_not_repaint_the_unchanged_pixels_between_them() {
    let mut damage = Damage::default();
    let old = [sprite(-0.75), sprite(0.75)];
    let lights = [[1.0; 3]; 2];
    damage.remember(&old, &lights, [128.0; 2], [128; 2], false, &[]);
    let changed = [sprite(-0.7), sprite(0.7)];
    let regions = damage.regions(&changed, &lights, [128.0; 2], [128; 2], false, &[]);
    assert_eq!(regions.len(), 2);
    let pixels: u32 = regions.iter().map(|r| (r[2] - r[0]) * (r[3] - r[1])).sum();
    assert!(
        pixels < 128 * 128 / 20,
        "no large empty rectangle between actors"
    );
}

#[wasm_bindgen_test]
fn moving_across_depth_order_does_not_invalidate_unmoved_layers() {
    let mut damage = Damage::default();
    let moving = sprite(-0.75);
    let mut still = sprite(0.75);
    if let WorldLayer::Sprite(_, _, depth, id) = &mut still {
        *depth = 10.0;
        *id = 8;
    }
    let lights = [[1.0; 3]; 2];
    damage.remember(&[moving, still], &lights, [128.0; 2], [128; 2], false, &[]);
    let mut moved = sprite(-0.7);
    if let WorldLayer::Sprite(_, _, depth, _) = &mut moved {
        *depth = 20.0;
    }
    let regions = damage.regions(&[still, moved], &lights, [128.0; 2], [128; 2], false, &[]);
    assert_eq!(regions.len(), 1);
    assert!(
        regions[0][2] < 40,
        "the unchanged actor on the right is not damage"
    );
}

fn sprite(x: f32) -> WorldLayer {
    WorldLayer::Sprite(
        Sprite {
            position: [x, 0.0],
            radius: [0.125; 2],
            color: [1.0; 4],
            uv: [0.0; 4],
            depths: [0.0; 4],
            terrain_blend: [[0.0; 4]; 2],
            pages: [0; 4],
        },
        GameFrame {
            atlas: crate::AtlasAddress {
                page: 0,
                uv: [0.0; 4],
            },
            size: [16.0; 2],
            anchor: [8.0; 2],
        },
        0.0,
        7,
    )
}

#[wasm_bindgen_test]
fn changed_sprite_damage_covers_old_and_new_pixels_without_the_full_frame() {
    let mut damage = Damage::default();
    let lights = [[1.0; 3]];
    assert_eq!(
        damage.regions(&[sprite(0.0)], &lights, [128.0; 2], [128; 2], false, &[]),
        vec![[0, 0, 128, 128]]
    );
    damage.remember(&[sprite(0.0)], &lights, [128.0; 2], [128; 2], false, &[]);
    assert_eq!(
        damage.regions(&[sprite(0.0)], &lights, [128.0; 2], [128; 2], false, &[]),
        Vec::<[u32; 4]>::new()
    );
    assert_eq!(
        damage.regions(&[sprite(0.125)], &lights, [128.0; 2], [128; 2], false, &[]),
        vec![[56, 56, 80, 72]]
    );
    assert_eq!(
        damage.regions(&[], &[], [128.0; 2], [128; 2], false, &[]),
        vec![[56, 56, 72, 72]]
    );
    assert_eq!(
        damage.regions(
            &[sprite(0.0)],
            &[[0.5; 3]],
            [128.0; 2],
            [128; 2],
            false,
            &[]
        ),
        vec![[56, 56, 72, 72]]
    );
    assert_eq!(
        damage.regions(&[sprite(0.0)], &lights, [128.0; 2], [128; 2], true, &[]),
        vec![[0, 0, 128, 128]]
    );
    damage.invalidate();
    assert_eq!(
        damage.regions(&[sprite(0.0)], &lights, [128.0; 2], [128; 2], false, &[]),
        vec![[0, 0, 128, 128]]
    );
}

#[wasm_bindgen_test]
fn canvas_damage_invalidates_size_viewport_and_atlas_and_bounds_retained_layers() {
    let mut damage = Damage::default();
    let layers = [sprite(0.0)];
    let lights = [[1.0; 3]];
    damage.remember(&layers, &lights, [128.0; 2], [128; 2], false, &[]);
    assert_eq!(
        damage.regions(&layers, &lights, [256.0; 2], [128; 2], false, &[]),
        vec![[0, 0, 128, 128]]
    );
    assert_eq!(
        damage.regions(&layers, &lights, [128.0; 2], [64; 2], false, &[]),
        vec![[0, 0, 64, 64]]
    );
    assert_eq!(
        damage.regions(&layers, &lights, [128.0; 2], [128; 2], false, &[0]),
        vec![[0, 0, 128, 128]]
    );
    let oversized = vec![sprite(0.0); MAX_LAYERS + 1];
    let fog = vec![[1.0; 3]; oversized.len()];
    damage.remember(&oversized, &fog, [128.0; 2], [128; 2], false, &[]);
    assert!(damage.layers.is_empty());
    assert!(damage.resident_bytes() <= MAX_LAYERS * (std::mem::size_of::<WorldLayer>() + 12));
    assert_eq!(
        damage.regions(&layers, &lights, [128.0; 2], [128; 2], false, &[]),
        vec![[0, 0, 128, 128]]
    );
}

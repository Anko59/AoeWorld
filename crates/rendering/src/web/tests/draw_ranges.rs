//! Mixed ground must never make ordinary faces run the ground-mixing program.
//! Runs remain contiguous so equal-depth alpha and sprite order cannot change.
use super::*;

fn mixed() -> Sprite {
    let mut sprite = solid_sprite();
    sprite.color[2] = 6.0;
    sprite.color[3] = -3.0;
    sprite.terrain_blend[0][2] = 1.0;
    sprite.pages[3] = 1;
    sprite
}

#[wasm_bindgen_test]
fn shader_runs_cover_every_instance_once_without_reordering() {
    let (plain, mix) = (solid_sprite(), mixed());
    let input = [plain, plain, mix, mix, plain, mix, plain];
    let ranges = draw_ranges(&input);
    assert_eq!(
        ranges,
        vec![
            (0..2, false),
            (2..4, true),
            (4..5, false),
            (5..6, true),
            (6..7, false)
        ]
    );
    assert_eq!(draw_ranges(&[]), vec![]);
    assert_eq!(draw_ranges(&[plain; 32]), vec![(0..32, false)]);
    assert_eq!(draw_ranges(&[mix; 32]), vec![(0..32, true)]);
}

#[wasm_bindgen_test]
fn only_actual_tiled_ground_mix_packets_choose_the_mixing_program() {
    let mix = mixed();
    for mode in [1.0, 2.0, 3.0] {
        let mut packet = mix;
        packet.terrain_blend[0][2] = mode;
        assert_eq!(draw_ranges(&[packet]), vec![(0..1, true)]);
    }
    let mut candidates = [mix; 5];
    candidates[0].color[3] = 1.0; // Ordinary sprites, even with spare-word garbage.
    candidates[1].color[3] = -2.0; // Solid surfaces, including the grid.
    candidates[2].color[2] = 4.0; // Non-tiled material triangles.
    candidates[3].pages[3] = 0; // Metadata-free and water blends.
    candidates[4].terrain_blend[0][2] = 0.0; // Forest-floor-only blend.
    assert_eq!(draw_ranges(&candidates), vec![(0..5, false)]);
}

#[wasm_bindgen_test]
async fn production_submission_keeps_program_runs_and_equal_depth_painter_order() {
    use super::terrain_blend::{assert_pixel, read_pixel, surface_renderer_with_atlas};
    use crate::surface_mesh::mix::fixture;
    let mut renderer = surface_renderer_with_atlas(&fixture::atlas()).await;
    let faces = fixture::scene(0);
    let mix = faces
        .iter()
        .map(|face| surface_instance(face, [128.0; 2], 0.0))
        .find(|packet| draw_ranges(&[*packet]) == vec![(0..1, true)])
        .expect("fixture has a real ground-mixing face");
    // Equal-depth sprites cover the probe with a white atlas sample. Reversing
    // red/blue across a mixing run must change the last painter and alpha.
    renderer.queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &renderer._atlas,
            mip_level: 0,
            origin: wgpu::Origin3d { x: 0, y: 2, z: 2 },
            aspect: wgpu::TextureAspect::All,
        },
        &[255; 4],
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4),
            rows_per_image: Some(1),
        },
        wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
    );
    let red = Sprite {
        radius: [1.0; 2],
        uv: [0.0, 2.0 / 2048.0, 1.0 / 2048.0, 1.0 / 2048.0],
        pages: [2, 0, 0, 0],
        color: [1.0, 0.0, 0.0, 1.0],
        ..solid_sprite()
    };
    let blue = Sprite {
        color: [0.0, 0.0, 1.0, 0.5],
        ..red
    };
    let mix = Sprite {
        depths: [-1.0; 4],
        ..mix
    };
    for (sprites, expected) in [
        ([red, mix, blue], [127, 0, 128, 255]),
        ([blue, mix, red], [255, 0, 0, 255]),
    ] {
        let counters = renderer.render_sprites(&sprites).unwrap();
        assert!(counters.did_present);
        assert_eq!(counters.draw_calls, 3);
        assert_eq!(
            renderer.draws,
            vec![(0..1, false), (1..2, true), (2..3, false)]
        );
        assert!(renderer.mixed_pipeline.is_some());
        assert_pixel(read_pixel(&renderer, 3, [64, 64]).await, expected);
    }
    renderer.device.destroy();
}

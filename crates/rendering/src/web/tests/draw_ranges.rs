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

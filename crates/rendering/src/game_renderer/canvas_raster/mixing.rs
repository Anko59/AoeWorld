//! Ordinary mixed ground: determine weights before resolving or sampling frames.
use super::*;

pub(super) fn sample(
    triangle: &ProjectedSurfaceTriangle,
    atlas: &[u8],
    locals: [[f64; 2]; 3],
    sheets: [directory::Sheet; 3],
    barycentric: [f64; 3],
    gradient: [[f64; 2]; 2],
    floors: Option<[f32; 3]>,
) -> [u8; 4] {
    let position = [0, 1].map(|axis| {
        locals[0][axis] * barycentric[0]
            + locals[1][axis] * barycentric[1]
            + locals[2][axis] * barycentric[2]
    });
    let cell = position.map(f64::floor);
    let inside = [0, 1].map(|axis| (position[axis] - cell[axis]) as f32);
    let tile = [0, 1].map(|axis| triangle.texture_tile[axis].wrapping_add(cell[axis] as i32));
    let local = directory::diamond([position[0] - cell[0], position[1] - cell[1]]);
    let share = |from: usize| {
        (0..3)
            .map(|i| f32::from(triangle.mix[from + i]) / 255.0 * barycentric[i] as f32)
            .sum::<f32>()
    };
    let bed = crate::surface_mesh::landscape::interpolated_floor_weights(
        floors,
        triangle.appearance,
        barycentric,
    )[1];
    let shares = [share(0), share(3), bed];
    let middle = triangle.mix[3..] != [0; 3];
    let noise = if mix::needs_noise(shares, middle) {
        mix::noise(tile, inside)
    } else {
        0.5
    };
    let weights = mix::weights(shares, middle, noise);
    let materials = triangle.texture_materials.unwrap_or([triangle.material; 3]);
    let mut samples = [[0.0; 4]; 3];
    for index in 0..3 {
        if weights[index] <= 0.0 {
            continue;
        }
        let frame = sheets[index].frame(atlas, tile);
        samples[index] = mix::layer(
            filter::sample(atlas, frame, local, filter::offsets(frame, gradient)),
            materials[index],
        );
    }
    crate::surface_mesh::landscape::blend(samples, weights, triangle.tint, triangle.appearance)
}

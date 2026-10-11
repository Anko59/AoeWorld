//! Junction sampling from shared vertex observations, in global layer order.
use super::*;

pub(super) fn sample(
    triangle: &ProjectedSurfaceTriangle,
    atlas: &[u8],
    packed: [u16; 3],
    palette: [u8; 12],
    barycentric: [f64; 3],
    gradient: [[f64; 2]; 2],
    floors: Option<[f32; 3]>,
) -> [u8; 4] {
    let locals = directory::locals(triangle);
    let position = [0, 1].map(|axis| {
        (0..3)
            .map(|i| locals[i][axis] * barycentric[i])
            .sum::<f64>()
    });
    let cell = position.map(f64::floor);
    let inside = [0, 1].map(|axis| (position[axis] - cell[axis]) as f32);
    let tile = [0, 1].map(|axis| triangle.texture_tile[axis].wrapping_add(cell[axis] as i32));
    let local = directory::diamond(inside.map(f64::from));
    let weights = mix::support::weights(packed, palette, barycentric, mix::noise(tile, inside));
    let bed = crate::surface_mesh::landscape::interpolated_floor_weights(
        floors,
        triangle.appearance,
        barycentric,
    )[1];
    let mut color = [0.0; 4];
    let mut accumulate = |material: u8, weight: f32| {
        if weight <= 0.0 {
            return;
        }
        let Some(sheet) = directory::sheet(atlas, material) else {
            return;
        };
        let frame = sheet.frame(atlas, tile);
        let texel = mix::layer(
            filter::sample(atlas, frame, local, filter::offsets(frame, gradient)),
            material,
        );
        for channel in 0..4 {
            color[channel] += texel[channel] * weight;
        }
    };
    for material in palette.into_iter().filter(|m| *m != 15) {
        accumulate(material, weights[usize::from(material)] * (1.0 - bed));
    }
    accumulate(
        triangle
            .texture_materials
            .map_or(6, |materials| materials[1]),
        bed,
    );
    crate::surface_mesh::landscape::blend(
        [color, [0.0; 4], [0.0; 4]],
        [1.0, 0.0, 0.0],
        triangle.tint,
        triangle.appearance,
    )
}

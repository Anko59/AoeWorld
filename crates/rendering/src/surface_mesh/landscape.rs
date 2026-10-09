//! Shared compact appearance packet and byte-rounding reference kernel.
use crate::SceneTerrainAppearance;

pub(crate) fn pack(value: Option<SceneTerrainAppearance>) -> u32 {
    value.map_or(0, |v| {
        1 | (u32::from(v.palette.min(5)) << 1)
            | (u32::from(v.floor_strength.min(1000)) << 4)
            | (u32::from(v.canopy_strength.min(1000)) << 14)
            | (u32::from(v.exposure.min(2)) << 24)
            | (u32::from(v.height_band.min(4)) << 26)
    })
}

/// Packet variant flag; metadata bits0..28 retain their original meaning.
pub(crate) const INTERPOLATED_FLOOR: u32 = 1 << 29;

#[inline(never)]
pub(crate) fn quantized_floor(value: u16) -> u8 {
    ((u32::from(value.min(1000)) * 255 + 500) / 1000) as u8
}

/// Numeric integer conversion (NOT bitcast): every 24-bit value is exact in f32.
pub(crate) fn pack_floors(values: [u8; 3]) -> f32 {
    let [a, b, c] = values.map(u32::from);
    (a | (b << 8) | (c << 16)) as f32
}

pub(crate) fn interpolated_floor_weights(
    values: Option<[f32; 3]>,
    word: u32,
    barycentric: [f64; 3],
) -> [f32; 3] {
    let Some(values) = values else {
        return floor_weights(word);
    };
    let floor = values[0] * barycentric[0] as f32
        + values[1] * barycentric[1] as f32
        + values[2] * barycentric[2] as f32;
    [1.0 - floor, floor, 0.0]
}

pub(crate) fn floor_weights(word: u32) -> [f32; 3] {
    let floor = ((word >> 4) & 1023).min(1000) as f32 / 1000.0;
    [1.0 - floor, floor, 0.0]
}

/// Vegetative-only transform. Procedural rock/snow/ice/water keep their exact
/// existing kernels. Integer packet decoding and one byte rounding match GPUs.
#[inline(never)]
pub(crate) fn texel(samples: [[u8; 4]; 3], weights: [f32; 3], tint: u8, word: u32) -> [u8; 4] {
    let palette = ((word >> 1) & 7).min(5) as usize;
    let scales = [
        [990, 1000, 970],
        [950, 1000, 980],
        [940, 1000, 930],
        [1040, 980, 880],
        [1030, 1000, 900],
        [1000, 1000, 1000],
    ];
    let canopy = ((word >> 14) & 1023).min(1000) as f32 / 1000.0;
    let shade = match tint {
        1 => 0.92,
        2 => 0.78,
        3 => 0.72,
        _ => 1.0,
    };
    let mut result = [0; 4];
    for c in 0..4 {
        let mut value = f32::from(samples[0][c]) * weights[0]
            + f32::from(samples[1][c]) * weights[1]
            + f32::from(samples[2][c]) * weights[2];
        if c < 3 {
            value *= shade * (scales[palette][c] as f32 / 1000.0) * (1.0 - 0.12 * canopy);
        }
        // The saturating cast already clamps finite overflow and infinities,
        // and maps NaN to zero; retain the sole rounding before conversion.
        result[c] = value.round() as u8;
    }
    result
}

//! Directional slope shading: ground facing the light is brighter, ground
//! facing away darker, flat ground unchanged. One kernel for all renderers.
//!
//! The light comes from the map's north-west, the top of the isometric screen,
//! as in the original game and the server's minimap. The slope is a local
//! plane fitted to the displayed heights around each cell (`lod::shade`), in
//! height levels per tile, so the one-level steps of the integer terrain read
//! as hillsides instead of faceted tents, and averaged far-zoom cells agree
//! with near-zoom tiles. The shade is per vertex (the mean of the cells
//! around a lattice corner, so both faces at a corner agree) and interpolated
//! across each face by the shaders and the Canvas raster. Each vertex carries a
//! signed four-bit code: the low mantissa bits of the face's `color`, flagged
//! in `order`, and the packet carries them above the footprint wear
//! (`Sprite::color.z`); no new struct fields, buffers or passes.

/// Brightness change per code step (4 %).
pub(crate) const STEP: f32 = 0.04;
/// Code range of a vertex: -8 to 7 steps, at most -32 % and +28 %.
pub(crate) const MIN_CODE: i8 = -8;
pub(crate) const MAX_CODE: i8 = 7;
/// Slope (levels per tile along the light) that reaches half of `MAX_CODE`.
const HALF_SLOPE: f64 = 0.08;
/// First bit of the three vertex codes in `Sprite::color.z`.
pub(crate) const PACKET_SCALE: f32 = 4096.0;

/// Shade of a ground slope in code steps (not yet rounded), given its height
/// gradient in levels per tile (east, south). Zero on flat ground; positive
/// when the ground faces the light, i.e. rises toward the south-east.
pub(crate) fn steps(gradient: [f64; 2]) -> f32 {
    let along = (gradient[0] + gradient[1]) * std::f64::consts::FRAC_1_SQRT_2;
    let shade = along / (along.abs() + HALF_SLOPE);
    (shade * f64::from(MAX_CODE)) as f32
}

/// The nearest vertex code.
pub(crate) fn quantize(steps: f32) -> i8 {
    steps
        .round()
        .clamp(f32::from(MIN_CODE), f32::from(MAX_CODE)) as i8
}

/// Multiplier of the lit colour for a (possibly interpolated) code.
pub(crate) fn factor(code: f32) -> f32 {
    1.0 + code * STEP
}

/// Three vertex codes as the twelve bits above the footprint wear.
pub(crate) fn packed(codes: [i8; 3]) -> u32 {
    codes
        .iter()
        .enumerate()
        .map(|(index, code)| u32::from(*code as u8 & 15) << (4 * index))
        .sum()
}

/// Shades one sampled texel exactly as both GPU fragment shaders do.
pub(crate) fn shade_sample(texel: [u8; 4], code: f32) -> [u8; 4] {
    if code == 0.0 {
        return texel;
    }
    let factor = factor(code);
    [
        (f32::from(texel[0]) * factor).round() as u8,
        (f32::from(texel[1]) * factor).round() as u8,
        (f32::from(texel[2]) * factor).round() as u8,
        texel[3],
    ]
}

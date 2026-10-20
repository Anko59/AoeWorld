//! Player-plane composition shared by the client and local previews.

/// Blend a selected team shade (ramp 0-7) over straight RGBA colour. DE supplies
/// the mask's intensity as `weight`; trial player pixels supply 255 and keep
/// their opaque recolouring. Transparent background RGB does not bleed in.
pub fn blend_player(base: [u8; 4], ramp: u8, weight: u8, colour: [u8; 3]) -> [u8; 4] {
    if weight == 0 {
        return base;
    }
    let shade = 0.65 + f32::from(ramp.min(7)) / 7.0 * 0.35;
    let mut result = [
        (f32::from(colour[0]) * shade) as u8,
        (f32::from(colour[1]) * shade) as u8,
        (f32::from(colour[2]) * shade) as u8,
        255,
    ];
    if weight == 255 {
        return result;
    }
    let foreground = u32::from(weight) * 255;
    let background = u32::from(base[3]) * (255 - u32::from(weight));
    let coverage = foreground + background;
    for (channel, base) in result[..3].iter_mut().zip(base) {
        *channel =
            ((u32::from(*channel) * foreground + u32::from(base) * background + coverage / 2)
                / coverage) as u8;
    }
    result[3] = ((coverage + 127) / 255) as u8;
    result
}

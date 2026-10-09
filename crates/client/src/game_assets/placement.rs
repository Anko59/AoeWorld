//! Source validation and page-aware copies; runtime and source pages never alias.
use super::*;
use manifest::{FrameRecord, Manifest};

pub(super) fn plan(
    manifest: &Manifest,
    selected: &[usize],
    domains: &[AtlasDomain],
) -> Result<Vec<Placement>, JsValue> {
    if selected.len() > MAX_SELECTED_FRAMES || selected.len() != domains.len() {
        return Err(JsValue::from_str("Invalid selected frame count"));
    }
    // Fixed scratch bounds metadata allocation even for malformed source input.
    let mut extents = [FrameExtent {
        domain: AtlasDomain::Objects,
        width: 0,
        height: 0,
    }; MAX_SELECTED_FRAMES];
    for (semantic, (&index, &domain)) in selected.iter().zip(domains).enumerate() {
        let frame = manifest.frames.get(index).ok_or("Invalid selected frame")?;
        let source = manifest
            .pages
            .get(usize::from(frame.page))
            .ok_or("Invalid sprite page")?;
        if u32::from(source.width) != GAME_ATLAS_SIDE
            || u32::from(source.height) != GAME_ATLAS_SIDE
            || u32::from(frame.x) + u32::from(frame.width) > GAME_ATLAS_SIDE
            || u32::from(frame.y) + u32::from(frame.height) > GAME_ATLAS_SIDE
        {
            return Err(JsValue::from_str("Invalid sprite bounds"));
        }
        extents[semantic] = FrameExtent {
            domain,
            width: frame.width,
            height: frame.height,
        };
    }
    pack_frames(&extents[..selected.len()]).map_err(error)
}

pub(super) fn record(frame: &FrameRecord, placed: Placement) -> GameFrame {
    let side = GAME_ATLAS_SIDE as f32;
    GameFrame {
        atlas: AtlasAddress {
            page: u32::from(placed.page),
            uv: [
                placed.x as f32 / side,
                placed.y as f32 / side,
                placed.width as f32 / side,
                placed.height as f32 / side,
            ],
        },
        size: [frame.width as f32, frame.height as f32],
        anchor: [frame.anchor_x as f32, frame.anchor_y as f32],
    }
}

/// Source sheets that compose one runtime texel, decoded one at a time so only
/// one 16 MiB source page is resident beside the runtime atlas.
#[derive(Clone, Copy)]
pub(super) enum Plane {
    /// Copied unconditionally; transparent texels are replaced by `Shadow`.
    Color,
    /// Overrides any texel whose player mask is visible.
    Player,
    /// Fills only texels that are still transparent after colour and player.
    Shadow,
}

/// Applies one plane. Running `Color`, `Player`, then `Shadow` reproduces the
/// per-texel precedence player mask, visible colour, then shadow.
pub(super) fn copy_plane(
    frame: &FrameRecord,
    placed: Placement,
    plane: Plane,
    source: &[u8],
    pixels: &mut [u8],
) -> Result<(), JsValue> {
    let side = GAME_ATLAS_SIDE as usize;
    if source.len() != GAME_ATLAS_PAGE_BYTES
        || pixels.len() != GAME_ATLAS_BYTES
        || usize::from(placed.page) >= GAME_ATLAS_BYTES / GAME_ATLAS_PAGE_BYTES
        || placed.width != frame.width
        || placed.height != frame.height
        || usize::from(frame.x) + usize::from(frame.width) > side
        || usize::from(frame.y) + usize::from(frame.height) > side
        || usize::from(placed.x) + usize::from(placed.width) > side
        || usize::from(placed.y) + usize::from(placed.height) > side
    {
        return Err(JsValue::from_str("Invalid atlas copy bounds"));
    }
    let base = usize::from(placed.page) * GAME_ATLAS_PAGE_BYTES;
    for row in 0..usize::from(frame.height) {
        for col in 0..usize::from(frame.width) {
            let from = ((usize::from(frame.y) + row) * side + usize::from(frame.x) + col) * 4;
            let to =
                base + ((usize::from(placed.y) + row) * side + usize::from(placed.x) + col) * 4;
            let mut value = [
                source[from],
                source[from + 1],
                source[from + 2],
                source[from + 3],
            ];
            match plane {
                Plane::Color => {}
                Plane::Player if value[3] > 0 => {
                    let shade = 0.65 + f32::from(value[0].min(7)) / 7.0 * 0.35;
                    value = [
                        (65.0 * shade) as u8,
                        (145.0 * shade) as u8,
                        (245.0 * shade) as u8,
                        255,
                    ];
                }
                Plane::Shadow if pixels[to + 3] == 0 => {}
                Plane::Player | Plane::Shadow => continue,
            }
            pixels[to..to + 4].copy_from_slice(&value);
        }
    }
    Ok(())
}

/// Test composition of the production plane passes in loader order.
#[cfg(test)]
pub(super) fn copy(
    frame: &FrameRecord,
    placed: Placement,
    color: &[u8],
    player: &[u8],
    shadow: &[u8],
    pixels: &mut [u8],
) -> Result<(), JsValue> {
    copy_plane(frame, placed, Plane::Color, color, pixels)?;
    copy_plane(frame, placed, Plane::Player, player, pixels)?;
    copy_plane(frame, placed, Plane::Shadow, shadow, pixels)
}

#[cfg(test)]
#[path = "tests/placement.rs"]
mod tests;

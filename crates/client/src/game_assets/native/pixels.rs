//! Exact colour/owner composition into bounded, guttered upload tiles.
use super::*;
use aoe_assets::catalog::packing::residency::{GUTTER, TILE_SIDE};

pub(super) fn compose(
    planes: &[Vec<u8>; 3],
    p: Placement,
    owned: bool,
    team: [u8; 3],
) -> Result<[Vec<u8>; 2], JsValue> {
    let size = [p.width, p.height];
    let expected = usize::from(p.width) * usize::from(p.height) * 4;
    if size
        .into_iter()
        .any(|n| n == 0 || n > aoe_assets::catalog::packing::MAX_FRAME_EXTENT)
        || planes.iter().any(|plane| plane.len() != expected)
        || p.x % TILE_SIDE != GUTTER
        || p.y % TILE_SIDE != GUTTER
        || u32::from(p.x) + u32::from(p.width) + u32::from(GUTTER) > GAME_ATLAS_SIDE
        || u32::from(p.y) + u32::from(p.height) + u32::from(GUTTER) > GAME_ATLAS_SIDE
    {
        return Err(error("Invalid native upload planes"));
    }
    let width = usize::from((p.width + 2 * GUTTER).div_ceil(TILE_SIDE) * TILE_SIDE);
    let height = usize::from((p.height + 2 * GUTTER).div_ceil(TILE_SIDE) * TILE_SIDE);
    let mut output = [vec![0; width * height * 4], vec![0; width * height * 4]];
    if p.x == GUTTER && p.y == GUTTER {
        output[0][..4].copy_from_slice(&[255; 4]);
    }
    for row in 0..usize::from(p.height) {
        for col in 0..usize::from(p.width) {
            let from = (row * usize::from(p.width) + col) * 4;
            let to = ((row + usize::from(GUTTER)) * width + col + usize::from(GUTTER)) * 4;
            let pixel = &planes[0][from..from + 4];
            let mut colour = [pixel[0], pixel[1], pixel[2], pixel[3]];
            let ramp = planes[1][from];
            let weight = planes[1][from + 3];
            if owned {
                output[1][to..to + 4].copy_from_slice(&[ramp, weight, colour[3], 0]);
                colour[3] = ((u32::from(weight) * 255
                    + u32::from(colour[3]) * (255 - u32::from(weight))
                    + 127)
                    / 255) as u8;
            } else {
                colour = aoe_assets::pack::blend_player(colour, ramp, weight, team);
            }
            if colour[3] == 0 {
                colour.copy_from_slice(&planes[2][from..from + 4]);
            }
            output[0][to..to + 4].copy_from_slice(&colour);
        }
    }
    Ok(output)
}

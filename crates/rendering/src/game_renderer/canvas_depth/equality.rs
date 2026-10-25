//! Float words include packed texture metadata: NaN payloads are data, not motion.
use super::WorldLayer;
use crate::{AtlasAddress, GameFrame, ProjectedSurfaceTriangle};

impl PartialEq for WorldLayer {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Surface(a), Self::Surface(b)) => surface(a, b),
            (Self::Selection(a, ad), Self::Selection(b, bd)) => {
                bytemuck::bytes_of(a) == bytemuck::bytes_of(b) && ad.to_bits() == bd.to_bits()
            }
            (Self::Sprite(a, af, ad, ai), Self::Sprite(b, bf, bd, bi)) => {
                bytemuck::bytes_of(a) == bytemuck::bytes_of(b)
                    && frame(*af, *bf)
                    && ad.to_bits() == bd.to_bits()
                    && ai == bi
            }
            _ => false,
        }
    }
}

fn frame(a: GameFrame, b: GameFrame) -> bool {
    address(a.atlas, b.atlas)
        && a.size.map(f32::to_bits) == b.size.map(f32::to_bits)
        && a.anchor.map(f32::to_bits) == b.anchor.map(f32::to_bits)
}

fn address(a: AtlasAddress, b: AtlasAddress) -> bool {
    a.page == b.page && a.uv.map(f32::to_bits) == b.uv.map(f32::to_bits)
}

fn optional(a: Option<AtlasAddress>, b: Option<AtlasAddress>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => address(a, b),
        (None, None) => true,
        _ => false,
    }
}

fn surface(a: &ProjectedSurfaceTriangle, b: &ProjectedSurfaceTriangle) -> bool {
    a.points.iter().zip(&b.points).all(|(a, b)| {
        a.world.map(f64::to_bits) == b.world.map(f64::to_bits)
            && a.screen.x.to_bits() == b.screen.x.to_bits()
            && a.screen.y.to_bits() == b.screen.y.to_bits()
    }) && a.color.map(f32::to_bits) == b.color.map(f32::to_bits)
        && a.tile == b.tile
        && a.skirt == b.skirt
        && a.material == b.material
        && a.appearance == b.appearance
        && a.floor_strengths == b.floor_strengths
        && a.mix == b.mix
        && a.texture_mode == b.texture_mode
        && a.tint == b.tint
        && optional(a.texture_uv, b.texture_uv)
        && match (a.texture_blend, b.texture_blend) {
            (Some(a), Some(b)) => a.into_iter().zip(b).all(|(a, b)| address(a, b)),
            (None, None) => true,
            _ => false,
        }
        && a.texture_tile == b.texture_tile
        && a.texture_materials == b.texture_materials
        && a.pickable == b.pickable
        && a.order == b.order
}

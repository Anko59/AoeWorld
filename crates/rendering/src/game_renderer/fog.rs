//! Fog shares ground geometry and the 112-byte GPU packet. Its spare high
//! atlas-page byte carries three 2-bit vertex levels and a presence bit.
use super::*;
use std::collections::BTreeMap;

const LEVELS: [f32; 4] = [0.0, 0.45, 0.725, 1.0];

fn code(value: f32) -> u32 {
    LEVELS
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| (*a - value).abs().total_cmp(&(*b - value).abs()))
        .map_or(3, |(i, _)| i as u32)
}

pub(super) fn vertices(
    triangle: &ProjectedSurfaceTriangle,
    light: &dyn Fn([f64; 2]) -> f32,
) -> [f32; 3] {
    triangle
        .points
        .map(|p| LEVELS[code(light([p.world[0], p.world[1]])) as usize])
}

pub(crate) fn surface_packet(packet: &mut Sprite, values: [f32; 3]) {
    if values == [1.0; 3] {
        return;
    }
    let packed = code(values[0]) | code(values[1]) << 2 | code(values[2]) << 4;
    packet.pages[2] |= (128 | packed) << 24;
}

pub(super) fn shade_objects(
    layers: &mut [WorldLayer],
    art: &GameArt,
    resources: &[SceneResource],
    units: &[SceneUnit],
    animation: usize,
    light: &dyn Fn([f64; 2]) -> f32,
) {
    // One key type shares the map machinery while keeping unit IDs disjoint
    // from resource frames, even when their numeric IDs collide.
    let mut levels = BTreeMap::new();
    for r in resources {
        if let Some((body, shadow)) = world_sprites::resource_presentation_at(art, *r, animation) {
            levels.insert(frame_key(r.id, body), light(r.position));
            if let Some(shadow) = shadow {
                levels.insert(frame_key(r.id, shadow), light(r.position));
            }
        }
    }
    for u in units {
        // Owned units are already inside their own LOS. Remembered enemy
        // buildings use their last position and the same fog illumination.
        let mut factor = light(u.position);
        if let Some(side) = u.kind.building_side() {
            factor = 0.45;
            for y in 0..side as i32 {
                for x in 0..side as i32 {
                    factor = factor.max(light([
                        u.position[0] - side / 2.0 + f64::from(x) + 0.5,
                        u.position[1] - side / 2.0 + f64::from(y) + 0.5,
                    ]));
                }
            }
        }
        levels.insert((u64::from(u.id.0), None), factor);
    }
    for layer in layers {
        if let WorldLayer::Sprite(sprite, frame, _, id) = layer {
            let factor = levels
                .get(&frame_key(*id, *frame))
                .or_else(|| levels.get(&(*id, None)))
                .copied()
                .unwrap_or(1.0);
            for c in &mut sprite.color[..3] {
                *c *= factor;
            }
        }
    }
}

type ObjectKey = (u64, Option<(u32, [u32; 4])>);

fn frame_key(id: u64, frame: GameFrame) -> ObjectKey {
    (
        id,
        Some((frame.atlas.page, frame.atlas.uv.map(f32::to_bits))),
    )
}

pub(super) fn grid_segments(
    layers: &[WorldLayer],
    camera: SceneCamera,
    light: &dyn Fn([f64; 2]) -> f32,
) -> Vec<[aoe_core::ScreenPoint; 2]> {
    game_grid::grid_segments(
        layers.iter().filter_map(|layer| match layer {
            WorldLayer::Surface(t)
                if t.points
                    .iter()
                    .any(|p| light([p.world[0], p.world[1]]) > 0.0) =>
            {
                Some(t)
            }
            _ => None,
        }),
        camera.zoom,
    )
}

#[path = "tests/fog/shading.rs"]
#[cfg(test)]
mod tests;

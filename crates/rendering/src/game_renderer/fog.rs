//! Fog shares ground geometry and the 112-byte GPU packet. Its spare high
//! atlas-page byte carries three 2-bit vertex levels and a presence bit.
use super::*;

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
    let mut levels = FogFactors::default();
    for r in resources {
        if !art.environment.is_empty() && world_sprites::environment::present(art, *r) {
            world_sprites::environment::frames(art, *r, animation, &mut |f| {
                if let Some(f) = art.resident_frame(f) {
                    let slot = levels.index(r.id);
                    levels.resources.push(ResourceFactor {
                        body: frame_key(f),
                        shadow: None,
                        light: light(r.position),
                        next: levels.entries[slot].resource,
                    });
                    levels.entries[slot].resource = levels.resources.len();
                }
            });
            continue;
        }
        if let Some((body, shadow)) = world_sprites::resource_presentation_at(art, *r, animation) {
            let slot = levels.index(r.id);
            levels.resources.push(ResourceFactor {
                body: frame_key(body),
                shadow: shadow.map(frame_key),
                light: if r.kind == SceneResource::RALLY_FLAG {
                    1.0
                } else {
                    light(r.position)
                },
                next: levels.entries[slot].resource,
            });
            levels.entries[slot].resource = levels.resources.len();
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
        let slot = levels.index(u64::from(u.id.0));
        levels.entries[slot].unit = if u.visual.ghost() { 1.0 } else { factor };
    }
    for layer in layers {
        if let WorldLayer::Sprite(sprite, frame, _, id) = layer {
            let factor = levels.light(*id, *frame);
            for c in &mut sprite.color[..3] {
                *c *= factor;
            }
        }
    }
}

type FrameKey = (u32, [u32; 4]);
fn frame_key(frame: GameFrame) -> FrameKey {
    (frame.atlas.page, frame.atlas.uv.map(f32::to_bits))
}

// Reuse the existing non-generic integer-pair index rather than instantiating
// two BTreeMap implementations in WASM. Split IDs losslessly into two words.
// All storage is temporary and bounded by displayed resources + units. Each
// resource has one paired-frame record; duplicate IDs form a newest-first chain
// so frame-specific last writes retain their original precedence.
#[derive(Default)]
struct FogFactors {
    keys: crate::WorldKeyIndex,
    entries: Vec<Factor>,
    resources: Vec<ResourceFactor>,
}
struct Factor {
    unit: f32,
    resource: usize,
}
struct ResourceFactor {
    body: FrameKey,
    shadow: Option<FrameKey>,
    light: f32,
    next: usize,
}
fn id_key(id: u64) -> [i32; 2] {
    [id as i32, (id >> 32) as i32]
}
impl FogFactors {
    fn index(&mut self, id: u64) -> usize {
        let slot = self.keys.entry(id_key(id));
        if slot == self.entries.len() {
            self.entries.push(Factor {
                unit: 1.0,
                resource: 0,
            });
        }
        slot
    }
    fn light(&self, id: u64, frame: GameFrame) -> f32 {
        let Some(slot) = self.keys.get(id_key(id)) else {
            return 1.0;
        };
        let entry = &self.entries[slot];
        let key = frame_key(frame);
        let mut next = entry.resource;
        while next != 0 {
            let resource = &self.resources[next - 1];
            if resource.body == key || resource.shadow == Some(key) {
                return resource.light;
            }
            next = resource.next;
        }
        entry.unit
    }
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

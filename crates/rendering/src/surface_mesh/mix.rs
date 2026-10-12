//! Soft borders between every pair of ground materials.
//!
//! Each displayed vertex takes the share of every ground material among its
//! four incident source tiles. A quad whose corners see more than one
//! material draws layers ordered by `rank`: the base, an optional
//! middle material or the forest-floor bed, and the overlay. Shares pass
//! through a world-anchored noisy threshold, so a border is a soft irregular
//! band centred on the tile edge, identical from both sides and at every zoom.
//! Junctions retain every observation and evaluate cumulative weights in that
//! same order, so omitted zero-share layers cannot change an edge colour.
//! Displayed vertices only: no resident-world material map is built.
use super::*;
pub(crate) mod support;

#[path = "mix/tests/fixture.rs"]
#[cfg(test)]
pub(crate) mod fixture;

const MISSING: u8 = u8::MAX;
/// Threshold gain and noise reach. `REACH <= GAIN - 1` keeps a share of zero
/// at zero and a share of one at one whatever the noise says.
const GAIN: f32 = 4.0;
const REACH: f32 = 2.6;

/// Overlay order, low to high: a higher material spills over a lower one.
/// Bare hard ground is lowest, vegetation creeps over it and snow covers all:
/// ice, rock, gravel, mud, sand, dirt, grass and dirt mix, dry grass, worn
/// grass, grass, lush grass, snow.
pub(crate) fn rank(material: u8) -> u8 {
    match material {
        8 => 0,
        4 => 1,
        16 => 2,
        9 => 3,
        3 => 4,
        2 => 5,
        13 => 6,
        1 => 7,
        14 => 8,
        0 => 9,
        15 => 10,
        7 => 11,
        _ => 12,
    }
}

/// Materials that carry the ecological palette and the forest-floor bed.
pub(crate) fn vegetative(material: u8) -> bool {
    matches!(material, 0 | 1 | 2 | 6 | 13..=15)
}

impl ProjectedSurfaceTriangle {
    /// Faces drawn through the appearance word: palette, bed and filtered sheets.
    pub(crate) fn vegetated(&self) -> bool {
        vegetative(self.material) || self.mix != [0; 6] || self.appearance & support::FULL != 0
    }
}

/// The sheet a land tile shows and its forest-floor strength. Water
/// and metadata-free samples are not observations.
fn ground(sample: &SceneTerrain) -> Option<(u8, u16)> {
    let appearance = sample.appearance?;
    if sample.surface.water != 0 || appearance::water_material(sample.material) {
        return None;
    }
    let material = match sample.material {
        0 | 6 => u8::from(matches!(appearance.palette, 3 | 4)),
        material => material,
    };
    let floor = if vegetative(sample.material) {
        appearance.floor_strength.min(1000)
    } else {
        0
    };
    Some((material, floor))
}

fn eligible(triangle: &ProjectedSurfaceTriangle) -> bool {
    triangle.appearance & 1 != 0
        && !appearance::water_material(triangle.material)
        && !triangle.skirt
        && triangle.tint <= 3
        && triangle.texture_materials.is_some()
}

fn vertex(point: SurfacePoint) -> [i32; 2] {
    [point.world[0].floor() as i32, point.world[1].floor() as i32]
}

pub(super) fn assign(triangles: &mut [ProjectedSurfaceTriangle], terrain: &[SceneTerrain]) {
    let mut keys = crate::WorldKeyIndex::default();
    let mut support: Vec<[(u8, u16); 4]> = Vec::new();
    for triangle in triangles.iter().filter(|triangle| eligible(triangle)) {
        for point in triangle.points {
            if keys.entry(vertex(point)) == support.len() {
                support.push([(MISSING, 0); 4]);
            }
        }
    }
    if support.is_empty() {
        return;
    }
    for sample in terrain {
        let Some(cell) = ground(sample) else {
            continue;
        };
        let tile = tile_key(sample.position);
        for dy in 0..=1 {
            for dx in 0..=1 {
                let (Some(x), Some(y)) = (tile[0].checked_add(dx), tile[1].checked_add(dy)) else {
                    continue;
                };
                if let Some(index) = keys.get([x, y]) {
                    let slot = &mut support[index][(dy * 2 + dx) as usize];
                    // Duplicate records cannot reweight a vertex; conflicting
                    // ones choose the lower material and floor, whatever the order.
                    if slot.0 == MISSING || (rank(cell.0), cell.1) < (rank(slot.0), slot.1) {
                        *slot = cell;
                    }
                }
            }
        }
    }
    for triangle in triangles.iter_mut().filter(|triangle| eligible(triangle)) {
        let cells = triangle
            .points
            .map(|point| support[keys.get(vertex(point)).expect("displayed mix vertex")]);
        // Both triangles span the same axis-aligned patch bounds. Inspect all
        // four quad corners, never frequency counts from a triangle's third vertex.
        let points = triangle.points.map(vertex);
        let bounds = [0, 1].map(|axis| {
            points.iter().fold([i32::MAX, i32::MIN], |bounds, point| {
                [bounds[0].min(point[axis]), bounds[1].max(point[axis])]
            })
        });
        let quad = [[0, 0], [1, 0], [1, 1], [0, 1]].map(|corner| {
            support[keys
                .get([bounds[0][corner[0]], bounds[1][corner[1]]])
                .expect("displayed quad corner")]
        });
        let seen = || {
            quad.iter()
                .flatten()
                .map(|cell| cell.0)
                .filter(|material| *material != MISSING)
        };
        let (Some(base), Some(over)) = (
            seen().min_by_key(|m| rank(*m)),
            seen().max_by_key(|m| rank(*m)),
        ) else {
            continue;
        };
        if base == over {
            continue;
        }
        let middle = seen()
            .filter(|material| *material != base && *material != over)
            .max_by_key(|material| rank(*material))
            .unwrap_or(base);
        let full = seen()
            .any(|material| material != base && material != middle && material != over)
            || (middle != base && quad.iter().flatten().any(|cell| cell.1 != 0));
        let mut floors = [0; 3];
        for (corner, cell) in cells.iter().enumerate() {
            let present = cell.iter().filter(|cell| cell.0 != MISSING).count() as u32;
            let share = |material: u8| {
                let matching = cell.iter().filter(|cell| cell.0 == material).count() as u32;
                (matching * 255 + present / 2)
                    .checked_div(present)
                    .unwrap_or(0) as u8
            };
            triangle.mix[corner] = share(over);
            triangle.mix[3 + corner] = if middle == base { 0 } else { share(middle) };
            let floor = cell.iter().map(|cell| u32::from(cell.1)).sum::<u32>();
            floors[corner] = landscape::quantized_floor(
                ((floor + present / 2).checked_div(present).unwrap_or(0)) as u16,
            );
        }
        if full {
            support::retain(triangle, cells);
        }
        triangle.texture_materials = Some([base, middle, over]);
        triangle.floor_strengths = Some(floors);
    }
}

/// Selects the three sheets of a mixed face. Such a face always samples by
/// world tile, so its packet has room for the shares and the noise is anchored.
pub(super) fn texture(triangle: &mut ProjectedSurfaceTriangle, art: &GameArt) -> bool {
    if triangle.mix == [0; 6] && triangle.appearance & support::FULL == 0 {
        return false;
    }
    let full = support::packed(triangle).is_some();
    let observations = triangle.texture_blend;
    let [base, middle, over] = triangle.texture_materials.unwrap_or([triangle.material; 3]);
    // Ordinary borders use three samples. Junctions keep the bed here and
    // retain all ground observations in the otherwise unused third rectangle.
    let bed = triangle
        .floor_strengths
        .is_some_and(|floors| floors != [0; 3]);
    let second = if full || bed || triangle.mix[3..] == [0; 3] {
        triangle.mix[3..].fill(0);
        if art.terrain[6].is_empty() { 2 } else { 6 }
    } else {
        middle
    };
    let frame = |material| {
        appearance::terrain_texture_frame(art, material, triangle.texture_tile).map(|f| f.atlas)
    };
    let (Some(a), Some(b), Some(c)) = (frame(base), frame(second), frame(over)) else {
        triangle.mix = [0; 6];
        triangle.appearance &= !support::FULL;
        triangle.texture_blend = None;
        return false;
    };
    triangle.texture_materials = Some([base, second, over]);
    triangle.texture_mode = directory::TILED;
    triangle.texture_uv = Some(a);
    triangle.texture_blend = Some([
        b,
        if full {
            observations.map_or(c, |addresses| addresses[1])
        } else {
            c
        },
    ]);
    if !vegetative(triangle.material) {
        // Bare ground keeps face shading but no ecological palette or canopy.
        triangle.appearance = (triangle.appearance & support::FULL) | 1 | 5 << 1;
    }
    true
}

fn lattice(x: i32, y: i32) -> f32 {
    let mut hash = (x as u32).wrapping_mul(2_654_435_761) ^ (y as u32).wrapping_mul(2_246_822_519);
    hash = (hash ^ (hash >> 15)).wrapping_mul(739_982_445);
    ((hash ^ (hash >> 12)) & 255) as f32 / 255.0
}

fn value(tile: [i32; 2], inside: [f32; 2], cells: i32) -> f32 {
    let scaled = inside.map(|v| v * cells as f32);
    let cell = scaled.map(f32::floor);
    let [x, y] = [0, 1].map(|axis| {
        tile[axis]
            .wrapping_mul(cells)
            .wrapping_add(cell[axis] as i32)
    });
    let [u, v] = [0, 1].map(|axis| {
        let t = scaled[axis] - cell[axis];
        t * t * (3.0 - 2.0 * t)
    });
    let (x1, y1) = (x.wrapping_add(1), y.wrapping_add(1));
    let (a, b, c, d) = (
        lattice(x, y),
        lattice(x1, y),
        lattice(x, y1),
        lattice(x1, y1),
    );
    let top = a + (b - a) * u;
    let bottom = c + (d - c) * u;
    top + (bottom - top) * v
}

/// World-anchored value noise in 0..=1: tile-sized meanders plus quarter-tile raggedness.
pub(crate) fn noise(tile: [i32; 2], inside: [f32; 2]) -> f32 {
    0.6 * value(tile, inside, 1) + 0.4 * value(tile, inside, 4)
}

fn edge(share: f32, noise: f32) -> f32 {
    if share <= 0.0 {
        return 0.0;
    }
    if share >= 1.0 {
        return 1.0;
    }
    ((share - 0.5) * GAIN + 0.5 + (noise - 0.5) * REACH).clamp(0.0, 1.0)
}

/// Pure shares and a complete forest bed are independent of the noise lattice.
pub(crate) fn needs_noise([over, mid, bed]: [f32; 3], middle: bool) -> bool {
    let partial = |share: f32| share > 0.0 && share < 1.0;
    (middle || bed < 1.0) && (partial(over) || (middle && partial(over + mid)))
}

/// Layer weights from the interpolated overlay share, middle share and bed.
pub(crate) fn weights(shares: [f32; 3], middle: bool, noise: f32) -> [f32; 3] {
    let [share, mid, bed] = shares;
    let over = edge(share, noise);
    if middle {
        let mid = edge(share + mid, noise);
        // Cumulative thresholds: a layer with no share here has no weight.
        [1.0 - mid, mid - over, over]
    } else {
        [(1.0 - bed) * (1.0 - over), bed, (1.0 - bed) * over]
    }
}

/// Preserve authored ice; snow uses its white kernel at borders and junctions.
pub(crate) fn art_layer(texel: [u8; 4], material: u8, appearance: u32, tint: u8) -> [f32; 4] {
    let mut sample = if appearance & super::landscape::NATIVE_ICE != 0 && material == 8 {
        texel.map(f32::from)
    } else {
        layer(texel, material)
    };
    // Snow is diffuse: compensate its layer before the shared ramp shade,
    // keeping rock/grass lighting and matching the pure snow tint's 0.98.
    if material == 7 && tint == 1 {
        for channel in &mut sample[..3] {
            *channel *= 0.98 / 0.92;
        }
    }
    sample
}

/// One layer's colour before blending: procedural rock, snow, ice and mud are
/// dirt or water detail on a neutral base, every other sheet is itself.
pub(crate) fn layer(texel: [u8; 4], material: u8) -> [f32; 4] {
    let texel = texel.map(f32::from);
    let (base, amount) = match material {
        4 => ([0.18; 3], 0.55),
        7 => ([0.82, 0.835, 0.85], 0.14),
        8 => ([0.42, 0.57, 0.65], 0.20),
        9 => ([0.10, 0.08, 0.05], 0.35),
        _ => return texel,
    };
    let detail = (texel[0] + texel[1] + texel[2]) / 3.0;
    [
        base[0] * 255.0 + detail * amount,
        base[1] * 255.0 + detail * amount,
        base[2] * 255.0 + detail * amount,
        texel[3],
    ]
}

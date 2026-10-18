//! Soft borders between ground materials and shallow shore water.
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
mod thin;

#[path = "mix/tests/fixture.rs"]
#[cfg(test)]
pub(crate) mod fixture;
#[path = "mix/tests/narrow_stream.rs"]
#[cfg(test)]
mod narrow_stream_tests;

const MISSING: u8 = u8::MAX;
const DEEP: u8 = 5;
/// Threshold gain and noise reach. `REACH <= GAIN - 1` keeps a share of zero
/// at zero and a share of one at one whatever the noise says.
const GAIN: f32 = 4.0;
const REACH: f32 = 2.6;

/// Overlay order, low to high: a higher material spills over a lower one.
/// Bare hard ground is lowest, vegetation creeps over it and snow covers all:
/// ice, rock, gravel, mud, sand, dirt, grass and dirt mix, dry grass, worn
/// grass, grass, lush grass, snow, shallow water and ford shallows.
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
        10 => 12,
        12 => 13,
        _ => 14,
    }
}

/// Materials that carry the ecological palette and the forest-floor bed.
pub(crate) fn vegetative(material: u8) -> bool {
    matches!(material, 0 | 1 | 2 | 6 | 13..=15)
}

impl ProjectedSurfaceTriangle {
    /// Faces drawn through the appearance word: palette, bed and filtered sheets.
    pub(crate) fn vegetated(&self) -> bool {
        vegetative(self.material)
            || self.mix != [0; 6]
            || self.appearance & support::FULL != 0
            || (self.floor_strengths.is_some()
                && self
                    .texture_materials
                    .is_some_and(|m| matches!(m[1], 10 | 12)))
    }
}

/// The displayed sheet and its forest-floor strength. Shallow water overlays
/// ground; loaded deeper water completes the shore halo but keeps its own faces.
pub(super) fn ground(sample: &SceneTerrain) -> Option<(u8, u16)> {
    let appearance = sample.appearance?;
    if sample.surface.water != 0 || appearance::water_material(sample.material) {
        return match sample.material {
            10 | 12 => Some((sample.material, 0)),
            5 | 11 => Some((DEEP, 0)),
            _ => None,
        };
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
        && (!appearance::water_material(triangle.material) || matches!(triangle.material, 10 | 12))
        // Neutral bank closures carry the same shared-edge coverage as the tops.
        && (!triangle.skirt || triangle.tint == 0)
        && triangle.tint <= 3
        && triangle.texture_materials.is_some()
}

fn vertex(point: SurfacePoint) -> [i32; 2] {
    [point.world[0].floor() as i32, point.world[1].floor() as i32]
}

pub(super) fn assign(triangles: &mut [ProjectedSurfaceTriangle], terrain: &[SceneTerrain]) {
    let mut keys = crate::WorldKeyIndex::default();
    let mut support: Vec<[(u8, u16); 4]> = Vec::new();
    let mut water_visible = false;
    for triangle in triangles.iter().filter(|triangle| eligible(triangle)) {
        water_visible |= appearance::water_material(triangle.material);
        for point in triangle.points {
            if keys.entry(vertex(point)) == support.len() {
                support.push([(MISSING, 0); 4]);
            }
        }
    }
    if support.is_empty() {
        return;
    }
    // One temporary byte per requested vertex, only when shallow water is
    // displayed. Interior water keeps its native depth interpolation.
    let mut shore_depths = if water_visible {
        vec![MISSING; support.len()]
    } else {
        Vec::new()
    };
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
    let thin = if water_visible {
        thin::thin_vertices(&keys, support.len(), terrain)
    } else {
        Vec::new()
    };
    let thin_at = |point: SurfacePoint| thin.get(keys.get(vertex(point)).unwrap_or(0)).copied();
    for triangle in triangles.iter_mut().filter(|triangle| eligible(triangle)) {
        let mut cells = triangle
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
        let mut quad = [[0, 0], [1, 0], [1, 1], [0, 1]].map(|corner| {
            support[keys
                .get([bounds[0][corner[0]], bounds[1][corner[1]]])
                .expect("displayed quad corner")]
        });
        // Loaded depth completes a shallow band's halo without starting a
        // new blend at a direct land/deep-water boundary.
        let shallow = quad.iter().flatten().any(|cell| matches!(cell.0, 10 | 12));
        for cell in quad.iter_mut().flatten().chain(cells.iter_mut().flatten()) {
            if cell.0 == DEEP {
                cell.0 = if shallow { 10 } else { MISSING };
            }
        }
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
        if base == over && base == 12 && quad.iter().flatten().all(|cell| cell.0 == 12) {
            // A ford interior is a light shallow over sand, as its banks are.
            let share = thin::FORD_SHARE;
            triangle.texture_materials = Some([3, 3, 12]);
            triangle.mix = [share, share, share, 0, 0, 0];
            triangle.floor_strengths = Some([0; 3]);
            continue;
        }
        if base == over {
            // Coverage owns pure quads too. The older single-owner splat can
            // disagree with this support, especially on a coarse patch.
            triangle.material = base;
            triangle.texture_materials = Some([base; 3]);
            continue;
        }
        // A shore needs a complete source halo: an unloaded cell must not be
        // interpreted as dry ground or make a lone water cell spill outwards.
        if matches!(over, 10 | 12) && quad.iter().flatten().any(|cell| cell.0 == MISSING) {
            continue;
        }
        if !shore_depths.is_empty() && matches!(over, 10 | 12) {
            // Shore mixing reaches pure shallows at the inner quad edge. Pin
            // its four shared vertices so the next native depth face starts
            // with that same sheet, rather than an abrupt darker depth band.
            for corner in [[0, 0], [1, 0], [1, 1], [0, 1]] {
                let index = keys
                    .get([bounds[0][corner[0]], bounds[1][corner[1]]])
                    .expect("displayed shore depth vertex");
                shore_depths[index] = shore_depths[index].min(over);
            }
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
            let mut over_share = share(over);
            if thin_at(triangle.points[corner]) == Some(true) && matches!(over, 10 | 12) {
                over_share = over_share.max(thin::THIN_SHARE);
            }
            triangle.mix[corner] = if over == 12 {
                over_share.min(thin::FORD_SHARE)
            } else {
                over_share
            };
            triangle.mix[3 + corner] = if middle == base { 0 } else { share(middle) };
            let floor = cell.iter().map(|cell| u32::from(cell.1)).sum::<u32>();
            floors[corner] = landscape::quantized_floor(
                ((floor + present / 2).checked_div(present).unwrap_or(0)) as u16,
            );
        }
        if full {
            if matches!(over, 10 | 12) {
                for (corner, cell) in cells.iter_mut().enumerate() {
                    if thin_at(triangle.points[corner]) == Some(true) {
                        thin::fill(cell);
                    }
                }
            }
            support::retain(triangle, cells);
        }
        // A quad can see an overlay only at the corner opposite this triangle.
        // Zero shares use the ordinary path, which interprets three different
        // materials as vertex ownership rather than coverage. Keep it pure.
        triangle.texture_materials = Some(if !full && triangle.mix == [0; 6] {
            [base; 3]
        } else {
            [base, middle, over]
        });
        triangle.floor_strengths = Some(floors);
    }
    if !shore_depths.is_empty() {
        for triangle in triangles.iter_mut().filter(|face| {
            !face.skirt
                && appearance::water_material(face.material)
                && face.floor_strengths.is_none()
        }) {
            let ford = triangle.material == 12;
            if let Some(materials) = triangle.texture_materials.as_mut() {
                for (corner, point) in triangle.points.iter().enumerate() {
                    if let Some(index) = keys.get(vertex(*point)) {
                        // A thin reach of deeper water inside a ford would draw
                        // as isolated medium-blue triangles: it starts from the
                        // shallow sheet at its thin vertices.
                        let depth = match shore_depths[index] {
                            MISSING if !ford && thin[index] => 10,
                            depth => depth,
                        };
                        if depth != MISSING {
                            materials[corner] = depth;
                        }
                    }
                }
            }
        }
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
    // A bare two-sheet shore uses the ordinary interpolated two-layer path.
    // Reuse the bed endpoint word for water coverage, avoiding a mixing program
    // and extra draw runs for the common sand/water border.
    let shore =
        !full && !bed && middle == base && matches!(over, 10 | 12) && !matches!(base, 4 | 7..=9);
    let second = if shore {
        over
    } else if full || bed || triangle.mix[3..] == [0; 3] {
        triangle.mix[3..].fill(0);
        if art.terrain[6].is_empty() { 2 } else { 6 }
    } else {
        middle
    };
    let frame = |material| {
        appearance::terrain_texture_frame(art, material, triangle.texture_tile).map(|f| f.atlas)
    };
    let third = if shore { base } else { over };
    let (Some(a), Some(b), Some(c)) = (frame(base), frame(second), frame(third)) else {
        triangle.mix = [0; 6];
        triangle.appearance &= !support::FULL;
        triangle.texture_blend = None;
        return false;
    };
    triangle.texture_materials = Some([base, second, third]);
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
    if shore {
        triangle.floor_strengths = Some([triangle.mix[0], triangle.mix[1], triangle.mix[2]]);
        triangle.mix = [0; 6];
        // Reserved palette 6 is neutral, with a narrow shore coverage band.
        triangle.appearance = 1 | 6 << 1;
    } else if !vegetative(triangle.material) {
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

/// Water coverage is linear on both ordinary shores and ground junctions.
/// The remaining ground shares retain their noisy borders under that coverage.
pub(crate) fn threshold(share: f32, water: f32, noise: f32) -> f32 {
    if share <= water {
        return edge(share, 0.5);
    }
    let cover = edge(water, 0.5);
    cover + (1.0 - cover) * edge((share - water) / (1.0 - water), noise)
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

/// Mountain sheets retain their authored colour on borders and junctions.
pub(crate) fn art_layer(texel: [u8; 4], material: u8, appearance: u32, tint: u8) -> [f32; 4] {
    let mut sample = if appearance & super::landscape::NATIVE_ICE != 0 && material == 8 {
        texel.map(f32::from)
    } else {
        layer(texel, material)
    };
    // Mixed faces keep the ordinary shade on grass/dirt. Cancel it only on
    // mountain layers so their authored relief matches neighbouring pure tops.
    if matches!(material, 4 | 7) {
        let shade = match tint {
            1 => 0.92,
            2 => 0.78,
            3 => 0.72,
            _ => 1.0,
        };
        for channel in &mut sample[..3] {
            *channel /= shade;
        }
    }
    sample
}

/// Ice and mud use bounded detail; native ground sheets retain their colours.
pub(crate) fn layer(texel: [u8; 4], material: u8) -> [f32; 4] {
    let texel = texel.map(f32::from);
    let (base, amount) = match material {
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

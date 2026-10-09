//! Native world-tile sampling; immutable per-triangle table view, no pixel cache.
use super::*;

const ROW_BYTES: usize = 8192;
const HEADER_BYTES: usize = 128;
const RECORD_BYTES: usize = 12;
const MAX_RECORDS: usize = 672;

#[derive(Clone, Copy)]
struct Group {
    base: usize,
    count: u32,
    columns: i32,
    rows: i32,
    kind: u8,
}

pub(super) struct WorldSampler<'a> {
    row: &'a [u8],
    groups: [Group; 2],
    duplicate: bool,
    origin: [i32; 2],
    footprint: [f64; 2],
    gradient: [[f64; 2]; 2],
}

impl<'a> WorldSampler<'a> {
    /// The loader validates the complete table once. Here only bounded schema,
    /// identity and consumed records are checked; never rescan/hash all frames.
    pub(super) fn new(
        atlas: &'a [u8],
        triangle: &ProjectedSurfaceTriangle,
        gradient: [[f64; 2]; 2],
    ) -> Option<Self> {
        let world = triangle.world_texture()?;
        let side = GAME_ATLAS_SIDE as usize;
        let start = 2 * side * side * 4 + ROW_BYTES;
        let row = atlas.get(start..start + ROW_BYTES)?;
        if row.get(..4)? != b"TLUT"
            || u32::from_le_bytes(row.get(4..8)?.try_into().ok()?) != 0x0703_0001
            || u32::from_le_bytes(row.get(12..16)?.try_into().ok()?) != world.checksum
            || row[10..12] != [0, 0]
        {
            return None;
        }
        let total = usize::from(word(row, 8)?);
        if total > MAX_RECORDS {
            return None;
        }
        let footprint = world.footprint.map(f64::from);
        if footprint.iter().any(|v| !v.is_finite() || *v <= 0.0)
            || gradient.iter().flatten().any(|v| !v.is_finite())
        {
            return None;
        }
        let groups = [
            group(row, world.groups[0], total)?,
            group(row, world.groups[1], total)?,
        ];
        let gradient = gradient.map(|[u, v]| [(u + v) * footprint[0], (v - u) * footprint[1]]);
        if gradient.iter().flatten().any(|v| !v.is_finite()) {
            return None;
        }
        Some(Self {
            row,
            groups,
            duplicate: world.groups[0] == world.groups[1],
            origin: triangle.texture_tile,
            footprint,
            gradient,
        })
    }

    /// Both layers fail closed together, retaining the exact old-rect fallback.
    pub(super) fn sample(&self, atlas: &[u8], local: [f64; 2]) -> Option<[[u8; 4]; 2]> {
        let q = [
            (local[0] + local[1] - 0.5) * self.footprint[0],
            (local[1] - local[0] + 0.5) * self.footprint[1],
        ];
        let primary = self.layer(atlas, self.groups[0], q)?;
        let secondary = if self.duplicate {
            primary
        } else {
            self.layer(atlas, self.groups[1], q)?
        };
        Some([primary, secondary])
    }

    fn layer(&self, atlas: &[u8], group: Group, q: [f64; 2]) -> Option<[u8; 4]> {
        let (center, extent) = self.nearest(atlas, group, q)?;
        // Different native dimensions are permitted. Derivatives are unwrapped
        // q, transformed through the native diamond and CENTER frame extent.
        let footprint = self
            .gradient
            .iter()
            .map(|[x, y]| {
                let u = 0.5 * (x - y) * extent[0];
                let v = 0.5 * (x + y) * extent[1];
                u.powi(2) + v.powi(2)
            })
            .fold(0.0_f64, f64::max);
        if !footprint.is_finite() {
            return None;
        }
        if footprint <= 1.5625 {
            return Some(center);
        }
        let mut sums = [0_u32; 4];
        for [sx, sy] in [[-0.25, -0.25], [-0.25, 0.25], [0.25, -0.25], [0.25, 0.25]] {
            let tap_q = std::array::from_fn(|axis| {
                q[axis] + sx * self.gradient[0][axis] + sy * self.gradient[1][axis]
            });
            // No local-UV clamp: each quadrant resolves its own world owner,
            // topology phase, native rectangle and page, even across a seam.
            let (tap, _) = self.nearest(atlas, group, tap_q)?;
            for channel in 0..3 {
                sums[channel] += u32::from(tap[channel]) * u32::from(tap[3]);
            }
            sums[3] += u32::from(tap[3]);
        }
        let mut result = center;
        if sums[3] != 0 {
            for channel in 0..3 {
                result[channel] = ((sums[channel] + sums[3] / 2) / sums[3]) as u8;
            }
        }
        // Coverage/depth always use nearest CENTER alpha, not averaged taps.
        Some(result)
    }

    fn nearest(&self, atlas: &[u8], group: Group, q: [f64; 2]) -> Option<([u8; 4], [f64; 2])> {
        // Canonical 16-bit phase prevents barycentric ULPs from changing nearest
        // coverage at world edges. Signed ties go toward +infinity, as on GPU.
        // Derivatives stay unwrapped; each center/filter tap snaps independently.
        let q = q.map(|v| (v * 65536.0 + 0.5).floor() / 65536.0);
        let floors = q.map(f64::floor);
        if floors
            .iter()
            .any(|v| !v.is_finite() || *v < f64::from(i32::MIN) || *v > f64::from(i32::MAX))
        {
            return None;
        }
        let owner = std::array::from_fn::<_, 2, _>(|axis| {
            self.origin[axis].wrapping_add(floors[axis] as i32)
        });
        let local_index = if group.kind == 1 {
            let column = owner[0].rem_euclid(group.columns) as u32;
            let row = (group.rows - owner[1].rem_euclid(group.rows)).rem_euclid(group.rows) as u32;
            column * group.rows as u32 + row
        } else {
            owner[0]
                .wrapping_mul(7)
                .wrapping_add(owner[1].wrapping_mul(13))
                .unsigned_abs()
                % group.count
        };
        let index = group.base.checked_add(local_index as usize)?;
        let offset = HEADER_BYTES.checked_add(index.checked_mul(RECORD_BYTES)?)?;
        let record = self.row.get(offset..offset + RECORD_BYTES)?;
        let x = word(record, 0)?;
        let y = word(record, 2)?;
        let width = word(record, 4)?;
        let height = word(record, 6)?;
        let page = record[8];
        if page > 1
            || x < 2
            || y < 2
            || width == 0
            || height == 0
            || u32::from(x) + u32::from(width) + 1 >= GAME_ATLAS_SIDE
            || u32::from(y) + u32::from(height) + 1 >= GAME_ATLAS_SIDE
            || record[9..12] != [0, 0, 0]
        {
            return None;
        }
        let r = std::array::from_fn::<_, 2, _>(|axis| q[axis] - floors[axis]);
        let uv = [0.5 + 0.5 * (r[0] - r[1]), 0.5 * (r[0] + r[1])];
        let extent = [f64::from(width - 1), f64::from(height - 1)];
        Some((
            sample_atlas(
                atlas,
                u32::from(page),
                (f64::from(x) + 0.5) + uv[0] * extent[0],
                (f64::from(y) + 0.5) + uv[1] * extent[1],
            ),
            extent,
        ))
    }
}

fn word(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn group(row: &[u8], id: u8, total: usize) -> Option<Group> {
    if id >= 7 {
        return None;
    }
    let offset = 16 + usize::from(id) * 12;
    let base = usize::from(word(row, offset)?);
    let count = u32::from(word(row, offset + 2)?);
    let columns = i32::from(word(row, offset + 4)?);
    let rows = i32::from(word(row, offset + 6)?);
    let kind = row[offset + 8];
    if count == 0
        || base.checked_add(count as usize)? > total
        || row[offset + 9..offset + 12] != [0, 0, 0]
        || !match kind {
            1 => columns > 0 && rows > 0 && columns as u32 * rows as u32 == count,
            2 => columns == 0 && rows == 0,
            _ => false,
        }
    {
        return None;
    }
    Some(Group {
        base,
        count,
        columns,
        rows,
        kind,
    })
}

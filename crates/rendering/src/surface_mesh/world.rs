//! Optional world sampling is enabled only by explicit catalog/layout provenance.
use super::*;
#[cfg(test)]
#[path = "world/tests/fixtures.rs"]
pub(crate) mod fixtures;
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct WorldTerrainTexture {
    pub checksum: u32,
    pub groups: [u8; 2],
    pub footprint: [f32; 2],
}

impl ProjectedSurfaceTriangle {
    /// Downgrade cached metadata before raw/unadmitted Canvas sampling.
    pub(crate) fn retain_world_texture(&mut self, admitted: Option<u32>) {
        let Some([primary, bed, marker]) = self.texture_materials else {
            return;
        };
        let Some([secondary, payload]) = self.texture_blend else {
            return;
        };
        if marker != (primary | 128) || primary > 2 || bed > 6 {
            return;
        }
        if self.world_texture().is_some() && admitted == Some(payload.page) {
            return;
        }
        self.texture_materials = Some([primary, bed, primary]);
        self.texture_blend = self
            .texture_uv
            .filter(|primary| *primary != secondary)
            .map(|primary| [secondary, primary]);
    }

    pub(crate) fn world_texture(&self) -> Option<WorldTerrainTexture> {
        let [primary, bed, marker] = self.texture_materials?;
        if marker != (primary | 128)
            || primary > 2
            || bed > 6
            || self.appearance & 1 == 0
            || self.skirt
            || self.tint > 3
            || self.texture_mode > 3
            || !matches!(self.material, 0 | 1 | 2 | 6)
        {
            return None;
        }
        let payload = self.texture_blend?[1];
        Some(WorldTerrainTexture {
            checksum: payload.page,
            groups: [primary, bed],
            footprint: [payload.uv[0], payload.uv[1]],
        })
    }
}

pub(super) fn qualify(
    t: &ProjectedSurfaceTriangle,
    art: &GameArt,
    groups: [u8; 2],
) -> Option<WorldTerrainTexture> {
    let checksum = art.terrain_world?;
    if t.appearance & 1 == 0
        || t.skirt
        || t.tint > 3
        || !matches!(t.material, 0 | 1 | 2 | 6)
        || t.texture_mode > 3
    {
        return None;
    }
    for group in groups {
        let frames = art.terrain.get(usize::from(group))?;
        let topology = art.terrain_topology[usize::from(group)]?;
        if frames.is_empty()
            || !match topology {
                crate::TerrainTopology::CoordinateStableAccents => true,
                periodic => periodic.periodic_frame(0, 0, frames.len()).is_some(),
            }
        {
            return None;
        }
    }
    let origin = t.texture_tile.map(f64::from);
    let mut span = [0.0_f64; 2];
    for point in t.points {
        for axis in 0..2 {
            span[axis] = span[axis].max(point.world[axis] - origin[axis]);
        }
    }
    if span.iter().any(|v| {
        !v.is_finite()
            || *v < 1.0
            || *v > f64::from(aoe_core::MAX_WORLD_DIMENSION_TILES)
            || v.fract() != 0.0
    }) {
        return None;
    }
    let corners = match t.texture_mode {
        0 => [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]],
        1 => [[0.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        2 => [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
        3 => [[1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        _ => return None,
    };
    for (point, corner) in t.points.iter().zip(corners) {
        for axis in 0..2 {
            if point.world[axis] != origin[axis] + corner[axis] * span[axis] {
                return None;
            }
        }
    }
    Some(WorldTerrainTexture {
        checksum,
        groups,
        footprint: span.map(|v| v as f32),
    })
}

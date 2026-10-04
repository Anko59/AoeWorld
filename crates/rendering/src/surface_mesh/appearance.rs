use super::*;

/// Only request vertices of the bounded displayed mesh. Never duplicate the
/// entire resident chunk set in a second art/material cache.
pub(super) fn assign_materials(
    triangles: &mut [ProjectedSurfaceTriangle],
    terrain: &[SceneTerrain],
) {
    let mut keys = crate::WorldKeyIndex::default();
    let mut materials = Vec::new();
    for triangle in triangles.iter().filter(|triangle| blendable(triangle)) {
        for point in triangle.points {
            let index = keys.entry(material_tile(point));
            if index == materials.len() {
                materials.push((triangle.material, None));
            } else {
                let value: &mut (u8, Option<u8>) = &mut materials[index];
                value.0 = value.0.min(triangle.material);
            }
        }
    }
    debug_assert!(materials.len() <= MAX_SURFACE_TRIANGLES * 3);
    for sample in terrain {
        if sample.surface.water == 0 && sample.surface.kind != SceneTerrainSurface::CLIFF {
            if let Some(index) = keys.get(tile_key(sample.position)) {
                materials[index].1 = Some(sample.material);
            }
        }
    }
    for triangle in triangles {
        if blendable(triangle) {
            let mut vertex_materials = [triangle.material; 3];
            for index in 0..3 {
                let point = triangle.points[index];
                if let Some(slot) = keys.get(material_tile(point)) {
                    let (fallback, source) = materials[slot];
                    vertex_materials[index] = source.unwrap_or(fallback);
                }
            }
            triangle.texture_materials = Some(vertex_materials);
        }
    }
}

fn blendable(triangle: &ProjectedSurfaceTriangle) -> bool {
    triangle.texture_materials.is_some()
}

fn material_tile(point: SurfacePoint) -> [i32; 2] {
    // Anchor material at source tile centres, with a single world-keyed owner
    // for shared vertices. This removes camera/chunk-order dependent borders.
    [
        (point.world[0] - 0.5).floor() as i32,
        (point.world[1] - 0.5).floor() as i32,
    ]
}

pub(super) fn texture_subdivisions(size: i32, cells: usize) -> i32 {
    let mut divisions = 1_i32;
    while divisions < size
        && cells
            .saturating_mul(8)
            .saturating_mul(divisions as usize)
            .saturating_mul(divisions as usize)
            <= MAX_SURFACE_TRIANGLES
    {
        divisions *= 2;
    }
    divisions
}

pub(crate) fn apply_terrain_textures(triangles: &mut [ProjectedSurfaceTriangle], art: &GameArt) {
    for triangle in triangles {
        let materials = triangle.texture_materials.unwrap_or([triangle.material; 3]);
        let mut frames = [None; 3];
        for index in 0..3 {
            frames[index] = terrain_texture_frame(art, materials[index], triangle.texture_tile)
                .map(|frame| frame.uv);
        }
        triangle.texture_uv = frames[0];
        triangle.texture_blend = match frames {
            [Some(a), Some(b), Some(c)] if a != b || a != c => Some([b, c]),
            _ => None,
        };
    }
}

pub(crate) fn terrain_texture_frame(
    art: &GameArt,
    material: u8,
    tile: [i32; 2],
) -> Option<GameFrame> {
    // Forest accents are bounded native variants, not a full periodic grid.
    // Mix with the existing complete dirt texture, identically in both backends.
    let material = if material == 6
        && (art.terrain[6].is_empty() || (tile[0].div_euclid(8) ^ tile[1].div_euclid(8)) & 1 == 0)
    {
        2
    } else {
        material
    };
    let frames = art
        .terrain
        .get(usize::from(material))
        .filter(|frames| !frames.is_empty())
        .unwrap_or(&art.grass);
    if frames.is_empty() {
        return None;
    }
    let index = if frames.len() == 100 {
        let y = (10 - tile[1].rem_euclid(10)).rem_euclid(10) as usize;
        tile[0].rem_euclid(10) as usize * 10 + y
    } else {
        tile[0]
            .wrapping_mul(7)
            .wrapping_add(tile[1].wrapping_mul(13))
            .unsigned_abs() as usize
            % frames.len()
    };
    frames.get(index).copied()
}

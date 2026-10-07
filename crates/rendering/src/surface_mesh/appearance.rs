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
        // Natural rock/cliffs and snow are procedural dirt-derived appearances,
        // never imported paving or ice. Keep their low-contrast transform local
        // to the whole face until per-material splat transforms are supported.
        if matches!(triangle.material, 4 | 7..=10) {
            triangle.texture_materials = None;
            let ramp = triangle.tint == 1 || matches!(triangle.tint, 21..=26);
            triangle.tint = match triangle.material {
                7 => 6,
                8 => 8,
                9 => 9,
                10 => 10,
                _ if triangle.skirt || matches!(triangle.tint, 7 | 23) => 7,
                _ if matches!(triangle.tint, 2 | 12) => 12,
                _ => 5,
            };
            if ramp {
                // Reserve base+16 codes without changing the surface packet ABI.
                triangle.tint += 16;
            }
        }
        if triangle.texture_materials.is_some_and(|materials| {
            materials
                .into_iter()
                .any(|material| matches!(material, 4 | 7..=10))
        }) {
            triangle.texture_materials = None;
        }
        if triangle.appearance & 1 != 0
            && matches!(triangle.material, 0 | 1 | 2 | 6)
            && triangle.tint <= 3
            && !triangle.skirt
        {
            let palette = (triangle.appearance >> 1) & 7;
            let dry = triangle.material == 1 || matches!(palette, 3 | 4);
            let primary = if dry { 1 } else { 0 };
            let a = terrain_texture_frame(art, primary, triangle.texture_tile).map(|f| f.atlas);
            let b = terrain_texture_frame(art, 6, triangle.texture_tile).map(|f| f.atlas);
            triangle.texture_materials = Some([primary, 6, primary]);
            triangle.texture_uv = a;
            triangle.texture_blend = a.zip(b).map(|(a, b)| [b, a]);
            continue;
        }
        let materials = triangle.texture_materials.unwrap_or([triangle.material; 3]);
        let mut frames = [None; 3];
        for index in 0..3 {
            frames[index] = terrain_texture_frame(art, materials[index], triangle.texture_tile)
                .map(|frame| frame.atlas);
        }
        triangle.texture_uv = frames[0];
        triangle.texture_blend = match frames {
            [Some(a), Some(b), Some(c)] if a != b || a != c => Some([b, c]),
            _ => None,
        };
    }
}

/// Procedural appearance kernel mirrored by both GPU fragment shaders.
/// Dirt/water luminance supplies bounded detail; no source-art identity changes.
pub(crate) fn procedural_tint(texel: [u8; 4], tint: u8) -> [u8; 4] {
    let ramp = matches!(tint, 21..=26);
    let tint = if ramp { tint - 16 } else { tint };
    let detail = (f32::from(texel[0]) + f32::from(texel[1]) + f32::from(texel[2])) / 3.0;
    let (base, amount, shade) = match tint {
        5 => ([0.18; 3], 0.55, 1.0),
        6 => ([0.78, 0.79, 0.78], 0.12, 1.0),
        7 => ([0.18; 3], 0.55, 0.72),
        8 => ([0.42, 0.57, 0.65], 0.20, 1.0),
        9 => ([0.10, 0.08, 0.05], 0.35, 1.0),
        10 => ([0.22, 0.36, 0.33], 0.25, 1.0),
        12 => ([0.18; 3], 0.55, 0.78),
        _ => return texel,
    };
    // Combine face lighting before the sole byte rounding, not after tinting.
    let shade = shade * if ramp { 0.92 } else { 1.0 };
    [
        ((base[0] * 255.0 + detail * amount) * shade).round() as u8,
        ((base[1] * 255.0 + detail * amount) * shade).round() as u8,
        ((base[2] * 255.0 + detail * amount) * shade).round() as u8,
        texel[3],
    ]
}

#[inline(never)]
pub(crate) fn terrain_texture_frame(
    art: &GameArt,
    material: u8,
    tile: [i32; 2],
) -> Option<GameFrame> {
    // Rock and snow deliberately reuse dirt detail, not paving or ice art.
    // Missing forest art uses dirt uniformly: no 8×8 parity substitution.
    let material = match material {
        4 | 7 | 9 => 2,
        8 | 10 => 5,
        6 if art.terrain[6].is_empty() => 2,
        _ => material,
    };
    let (frames, topology) = match art.terrain.get(usize::from(material)) {
        Some(frames) if !frames.is_empty() => (frames, art.terrain_topology[usize::from(material)]),
        _ => (&art.grass, art.terrain_topology[0]),
    };
    if frames.is_empty() {
        return None;
    }
    let index = topology
        .and_then(|topology| topology.periodic_frame(tile[0], tile[1], frames.len()))
        .unwrap_or_else(|| {
            tile[0]
                .wrapping_mul(7)
                .wrapping_add(tile[1].wrapping_mul(13))
                .unsigned_abs() as usize
                % frames.len()
        });
    frames.get(index).copied()
}

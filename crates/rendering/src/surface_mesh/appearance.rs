use super::*;

/// Only request vertices of the bounded displayed mesh. Never duplicate the
/// entire resident chunk set in a second art/material cache.
pub(super) fn assign_materials(
    triangles: &mut [ProjectedSurfaceTriangle],
    terrain: &[SceneTerrain],
) {
    // Land and water own separate vertex materials: water classes blend with
    // each other and land materials with each other, never across the shore.
    for wet in [false, true] {
        let mut keys = crate::WorldKeyIndex::default();
        let mut materials = Vec::new();
        for triangle in triangles.iter().filter(|triangle| blendable(triangle, wet)) {
            for point in triangle.points {
                let index = keys.entry(material_tile(point));
                if index == materials.len() {
                    materials.push((triangle.material, None));
                } else {
                    let value: &mut (u8, Option<u8>) = &mut materials[index];
                    value.0 = fallback(value.0, triangle.material);
                }
            }
        }
        debug_assert!(materials.len() <= MAX_SURFACE_TRIANGLES * 3);
        for sample in terrain {
            if water_material(sample.material) == wet
                && (wet || sample.surface.water == 0)
                && sample.surface.kind != SceneTerrainSurface::CLIFF
            {
                if let Some(index) = keys.get(tile_key(sample.position)) {
                    materials[index].1 = Some(sample.material);
                }
            }
        }
        for triangle in triangles.iter_mut() {
            if blendable(triangle, wet) {
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
}

fn blendable(triangle: &ProjectedSurfaceTriangle, wet: bool) -> bool {
    triangle.texture_materials.is_some() && water_material(triangle.material) == wet
}

/// Scene materials drawn with water art: medium 5, shallow 10, deep 11 and
/// ford shallows 12.
pub(crate) fn water_material(material: u8) -> bool {
    matches!(material, 5 | 10..=12)
}

/// Material of a vertex whose owning tile is not displayed or is across the
/// shore: the lowest land code, or the shallowest water class.
fn fallback(current: u8, candidate: u8) -> u8 {
    let rank = |material| match material {
        10 => 0,
        12 => 1,
        5 => 2,
        11 => 3,
        other => 4 + other,
    };
    if rank(candidate) < rank(current) {
        candidate
    } else {
        current
    }
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
        if mix::texture(triangle, art) {
            continue;
        }
        // Natural rock/cliffs and snow are procedural dirt-derived appearances,
        // never imported paving or ice. Keep their low-contrast transform local
        // to the whole face until per-material splat transforms are supported.
        if matches!(triangle.material, 4 | 7..=9) {
            triangle.texture_materials = None;
            let ramp = triangle.tint == 1 || matches!(triangle.tint, 21..=26);
            triangle.tint = match triangle.material {
                7 => 6,
                8 => 8,
                9 => 9,
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
                .any(|material| matches!(material, 4 | 7..=9))
        }) {
            triangle.texture_materials = None;
        }
        // A face over several tiles samples each tile's own frame, so its layers
        // differ whenever their sheets do, whatever the origin tile happens to show.
        let tiled = triangle.texture_mode == directory::TILED;
        let distinct = |first: u8, second: u8, a: &AtlasAddress, b: &AtlasAddress| {
            if tiled {
                terrain_group(art, first).map(|group| group.0)
                    != terrain_group(art, second).map(|group| group.0)
            } else {
                a != b
            }
        };
        if triangle.appearance & 1 != 0
            && matches!(triangle.material, 0 | 1 | 2 | 6)
            && triangle.tint <= 3
            && !triangle.skirt
        {
            let palette = (triangle.appearance >> 1) & 7;
            let dry = triangle.material == 1 || matches!(palette, 3 | 4);
            // Ecological palettes must not replace authoritative dirt with grass.
            let primary = if triangle.material == 2 {
                2
            } else if dry {
                1
            } else {
                0
            };
            let a = terrain_texture_frame(art, primary, triangle.texture_tile).map(|f| f.atlas);
            // Restore reviewed native leaf litter, retaining its coordinate-stable
            // accent selection, not claiming periodic seams. Packs without optional
            // forest art fall back to a dirt bed.
            let bed = if art.terrain[6].is_empty() { 2 } else { 6 };
            let b = terrain_texture_frame(art, bed, triangle.texture_tile).map(|f| f.atlas);
            triangle.texture_materials = Some([primary, bed, primary]);
            triangle.texture_uv = a;
            // Identical coherent addresses need one sample, not three reads of
            // the same bed (especially once each minified read is filtered).
            triangle.texture_blend = a
                .zip(b)
                .filter(|(a, b)| distinct(primary, bed, a, b))
                .map(|(a, b)| [b, a]);
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
            [Some(a), Some(b), Some(c)]
                if distinct(materials[0], materials[1], &a, &b)
                    || distinct(materials[0], materials[2], &a, &c) =>
            {
                Some([b, c])
            }
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

/// Frames and authored topology a scene material samples. Groups 0–6 are the
/// art's terrain groups, group 7 the grass fallback for an absent group and
/// groups 8–10 the optional water depth sheets.
pub(crate) fn terrain_group(
    art: &GameArt,
    material: u8,
) -> Option<(usize, &[GameFrame], Option<crate::TerrainTopology>)> {
    // Rock and snow deliberately reuse dirt detail, not paving or ice art.
    // Missing forest art uses dirt uniformly: no 8×8 parity substitution.
    // Water classes: the light shallow sheet is terrain group 5; medium,
    // deep and ford shallows use their optional sheet (groups 8-10), else
    // that same shallow sheet.
    let depth = match material {
        5 => Some(0),
        11 => Some(1),
        12 => Some(2),
        _ => None,
    }
    .filter(|slot: &usize| !art.water_depth[*slot].0.is_empty());
    if let Some(slot) = depth {
        let sheet = &art.water_depth[slot];
        return Some((8 + slot, &sheet.0, sheet.1));
    }
    let material = usize::from(match material {
        4 | 7 | 9 => 2,
        8 | 10..=12 => 5,
        6 if art.terrain[6].is_empty() => 2,
        _ => material,
    });
    match art.terrain.get(material) {
        Some(frames) if !frames.is_empty() => {
            Some((material, frames, art.terrain_topology[material]))
        }
        _ if !art.grass.is_empty() => Some((7, &art.grass, art.terrain_topology[0])),
        _ => None,
    }
}

/// The frame a world tile owns: authored sheet phase, else a coordinate hash.
pub(crate) fn frame_index(
    topology: Option<crate::TerrainTopology>,
    tile: [i32; 2],
    frames: usize,
) -> usize {
    topology
        .and_then(|topology| topology.periodic_frame(tile[0], tile[1], frames))
        .unwrap_or_else(|| {
            tile[0]
                .wrapping_mul(7)
                .wrapping_add(tile[1].wrapping_mul(13))
                .unsigned_abs() as usize
                % frames
        })
}

pub(crate) fn terrain_texture_frame(
    art: &GameArt,
    material: u8,
    tile: [i32; 2],
) -> Option<GameFrame> {
    let (_, frames, topology) = terrain_group(art, material)?;
    frames
        .get(frame_index(topology, tile, frames.len()))
        .copied()
}

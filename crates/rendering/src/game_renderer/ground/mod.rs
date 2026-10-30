//! Footprint wear and Farm textures share ground triangles across backends.
use super::*;

pub(super) struct BuildingGround {
    centers: Vec<[f64; 2]>,
    farms: Vec<[f64; 2]>,
}
/// Paint the DAT's foundation material plus a one-tile apron before tessellation.
/// The decoded building record has no margin field. The ordinary world-anchored
/// terrain mixer owns the apron border in all three backends. Water retains
/// its own sheet beneath coastal buildings, including Dock construction sites.
pub fn apply_building_foundations(
    terrain: &mut [SceneTerrain],
    art: &GameArt,
    units: &[SceneUnit],
) {
    if terrain.is_empty() || units.is_empty() {
        return;
    }
    // Compute the displayed sample bounds once, rather than scanning samples
    // for each distant building. Bounds include apron reach and half-open edges.
    let (mut min, mut max) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for tile in terrain.iter() {
        for axis in 0..2 {
            min[axis] = min[axis].min(tile.position[axis]);
            max[axis] = max[axis].max(tile.position[axis]);
        }
    }
    let foundations = units
        .iter()
        .filter_map(|unit| {
            if unit.visual.ghost()
                || unit.visual.moving()
                || unit.kind.rubble()
                || unit.kind.building_slot().is_some_and(|slot| slot >= 65)
            {
                return None;
            }
            let id = unit.kind.building_id()?;
            let building = art.compositions.iter().find(|b| b.unit_id == id);
            let side = unit.kind.building_side().unwrap_or(1.0) as f32;
            let footprint = building.map_or(if id == 792 { [4.0, 1.0] } else { [side; 2] }, |b| {
                b.footprint
            });
            let lower = [0, 1].map(|axis| unit.position[axis] - f64::from(footprint[axis]) / 2.0);
            let upper = [0, 1].map(|axis| unit.position[axis] + f64::from(footprint[axis]) / 2.0);
            if (0..2).any(|axis| upper[axis] + 1.0 <= min[axis] || lower[axis] - 1.0 > max[axis]) {
                return None;
            }
            let dirt = if id == 50 {
                art.compositions
                    .iter()
                    .find(|b| b.unit_id == 109)
                    .and_then(|b| b.foundation_material)
                    .unwrap_or(2)
            } else {
                building.and_then(|b| b.foundation_material).unwrap_or(2)
            };
            // A farm being built shows its own DAT foundation terrain (the
            // ploughed field, terrain 7) without crops; built farms add them.
            let crops = id == 50 && !unit.kind.is_foundation();
            Some(Foundation {
                lower,
                upper,
                apron: dirt,
                material: if id == 50 {
                    building.and_then(|b| b.foundation_material).unwrap_or(2)
                } else {
                    dirt
                },
                crops,
            })
        })
        .collect::<Vec<_>>();
    if foundations.is_empty() {
        return;
    }
    // Visit each displayed sample once. The last footprint wins over every
    // apron, independent of unit order, so neighbouring dirt cannot erase crops.
    for tile in terrain {
        if tile.surface.water != 0 || matches!(tile.material, 5 | 10) {
            continue;
        }
        let (mut footprint, mut apron) = (None, None);
        for foundation in &foundations {
            #[cfg(test)]
            tests::SAMPLE_VISITS.with(|count| count.set(count.get() + 1));
            if foundation.contains(tile.position, 0.0) {
                footprint = Some((foundation.material, foundation.crops));
            } else if foundation.contains(tile.position, 1.0) {
                apron = Some((foundation.apron, false));
            }
        }
        if let Some((material, crops)) = footprint.or(apron) {
            tile.material = material;
            // Crops keep per-triangle UVs; apron dirt participates in ordinary
            // landscape mixing with vegetation cleared and lighting preserved.
            tile.appearance = if crops {
                None
            } else {
                tile.appearance.map(|appearance| SceneTerrainAppearance {
                    canopy_strength: 0,
                    floor_strength: 0,
                    ..appearance
                })
            };
        }
    }
}

struct Foundation {
    lower: [f64; 2],
    upper: [f64; 2],
    material: u8,
    apron: u8,
    crops: bool,
}

impl Foundation {
    fn contains(&self, position: [f64; 2], margin: f64) -> bool {
        (0..2).all(|axis| {
            position[axis] >= self.lower[axis] - margin
                && position[axis] < self.upper[axis] + margin
        })
    }
}

impl BuildingGround {
    pub(super) fn new(units: &[SceneUnit], camera: SceneCamera) -> Self {
        let mut centers = Vec::new();
        for unit in units.iter().filter(|u| !u.visual.ghost()) {
            if unit.kind == SceneUnitKind::TownCenter {
                centers.push(unit.position);
            }
        }
        let projection = Camera {
            center: camera.center,
            zoom: camera.zoom,
            viewport: camera.viewport,
            focus_elevation_meters: camera.focus_elevation_meters,
        };
        let extent = [
            1.5 * aoe_core::ISO_TILE_WIDTH * camera.zoom,
            1.5 * aoe_core::ISO_TILE_HEIGHT * camera.zoom,
        ];
        let farms = units
            .iter()
            .filter(|u| u.kind == SceneUnitKind::Farm && !u.visual.ghost())
            // Cull once per redraw, before every terrain triangle tests candidates.
            // Include the full projected 3x3 footprint, even with its center offscreen.
            .filter(|u| {
                let p = projection.world_to_screen_at_height(u.position, u.elevation_meters);
                p.x + extent[0] >= 0.0
                    && p.y + extent[1] >= 0.0
                    && p.x - extent[0] <= camera.viewport[0]
                    && p.y - extent[1] <= camera.viewport[1]
            })
            .map(|u| u.position)
            .collect();
        Self { centers, farms }
    }

    pub(super) fn compatibility_farms(&self, camera: SceneCamera) -> Vec<ProjectedSurfaceTriangle> {
        if self.farms.is_empty() {
            return Vec::new();
        }
        let mut tiles = Vec::new();
        for p in &self.farms {
            for y in -1..=1 {
                for x in -1..=1 {
                    tiles.push(SceneTerrain {
                        position: [p[0] + f64::from(x), p[1] + f64::from(y)],
                        material: 2,
                        elevation_meters: 0.0,
                        surface: SceneTerrainSurface::flat(0.0),
                        appearance: None,
                    });
                }
            }
        }
        crate::projected_surface_triangles(&tiles, camera)
    }

    pub(super) fn farm(&self, face: &mut ProjectedSurfaceTriangle, art: &GameArt) {
        farm(face, &self.farms, art);
    }

    #[inline(never)]
    pub(super) fn apply(&self, face: &mut ProjectedSurfaceTriangle) {
        if self.centers.is_empty() || face.skirt {
            return;
        }
        let points = &face.points;
        let center = [
            (points[0].world[0] + points[1].world[0] + points[2].world[0]) / 3.0,
            (points[0].world[1] + points[1].world[1] + points[2].world[1]) / 3.0,
        ];
        let mut distance = f64::INFINITY;
        for building in &self.centers {
            let next = (center[0] - building[0])
                .abs()
                .max((center[1] - building[1]).abs());
            if next < distance {
                distance = next;
            }
        }
        let strength = ((3.4 - distance) / 1.4).clamp(0.0, 1.0) as f32;
        // One stable variation per source tile, shared by its two triangles.
        let hash = (face.tile[0] as u32).wrapping_mul(0x9e37_79b9)
            ^ (face.tile[1] as u32).wrapping_mul(0x85eb_ca6b);
        let strength = strength * (0.90 + (hash % 11) as f32 * 0.01);
        let [red, green, blue] = crate::surface_mesh::WEAR_DARKEN;
        face.scale_color([
            1.0 - red * strength,
            1.0 - green * strength,
            1.0 - blue * strength,
        ]);
        // Textured ground ignores `color`: the same strength rides the packet.
        face.set_wear((strength * f32::from(crate::surface_mesh::WEAR_LEVELS)).round() as u8);
    }
}

#[cfg(test)]
mod tests;

/// Replace the ground inside completed farms with the DE ploughed texture.
fn farm(face: &mut ProjectedSurfaceTriangle, farms: &[[f64; 2]], art: &GameArt) {
    if matches!(art.foundation.1, Some(crate::TerrainTopology::Dds { .. })) {
        return;
    }
    if face.skirt || farms.is_empty() {
        return;
    }
    let center = [
        (face.points[0].world[0] + face.points[1].world[0] + face.points[2].world[0]) / 3.0,
        (face.points[0].world[1] + face.points[1].world[1] + face.points[2].world[1]) / 3.0,
    ];
    let Some(farm) = farms
        .iter()
        .find(|p| (center[0] - p[0]).abs() < 1.5 && (center[1] - p[1]).abs() < 1.5)
    else {
        return;
    };
    let Some(frame) = art.buildings[5].first() else {
        if !art.foundation.0.is_empty() {
            return; // Prepared ground owns the native crop sheet and its borders.
        }
        face.texture_uv = None;
        face.texture_materials = None;
        face.texture_blend = None;
        face.texture_mode = 0;
        face.appearance = 0;
        face.tint = 0;
        face.scale_color([0.65, 0.45, 0.25]);
        return;
    };
    let mut atlas = frame.atlas;
    // Each ground tile samples a stable ninth of the farm's texture.
    let x = (center[0] - farm[0] + 1.5).floor().clamp(0.0, 2.0) as f32;
    let y = (center[1] - farm[1] + 1.5).floor().clamp(0.0, 2.0) as f32;
    atlas.uv[0] += atlas.uv[2] * x / 3.0;
    atlas.uv[1] += atlas.uv[3] * y / 3.0;
    atlas.uv[2] /= 3.0;
    atlas.uv[3] /= 3.0;
    face.texture_mode = face.mode().min(3);
    face.texture_uv = Some(atlas);
    face.texture_blend = None;
    face.texture_materials = None;
    face.tint = 0;
    face.appearance = 0;
    face.mix = [0; 6];
    face.floor_strengths = None;
}

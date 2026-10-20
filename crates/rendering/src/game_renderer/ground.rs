//! Footprint wear and Farm textures share ground triangles across backends.
use super::*;

pub(super) struct BuildingGround {
    centers: Vec<[f64; 2]>,
    farms: Vec<[f64; 2]>,
}

impl BuildingGround {
    pub(super) fn new(units: &[SceneUnit]) -> Self {
        let mut centers = Vec::new();
        for unit in units {
            if unit.kind == SceneUnitKind::TownCenter {
                centers.push(unit.position);
            }
        }
        let farms = units
            .iter()
            .filter(|u| u.kind == SceneUnitKind::Farm)
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

/// Replace the ground inside completed farms with the DE ploughed texture.
fn farm(face: &mut ProjectedSurfaceTriangle, farms: &[[f64; 2]], art: &GameArt) {
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

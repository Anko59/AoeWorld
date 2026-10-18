//! Footprint wear modulates the existing terrain; no decal textures or faces.
use super::*;

pub(super) struct BuildingGround(Vec<[f64; 2]>);

impl BuildingGround {
    pub(super) fn new(units: &[SceneUnit]) -> Self {
        let mut centers = Vec::new();
        for unit in units {
            if unit.kind == SceneUnitKind::TownCenter {
                centers.push(unit.position);
            }
        }
        Self(centers)
    }

    #[inline(never)]
    pub(super) fn apply(&self, face: &mut ProjectedSurfaceTriangle) {
        if self.0.is_empty() || face.skirt {
            return;
        }
        let points = &face.points;
        let center = [
            (points[0].world[0] + points[1].world[0] + points[2].world[0]) / 3.0,
            (points[0].world[1] + points[1].world[1] + points[2].world[1]) / 3.0,
        ];
        let mut distance = f64::INFINITY;
        for building in &self.0 {
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

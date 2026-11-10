//! Ground under a big building sinks behind its flat sprite.
use super::*;

/// The flat depth a building sprite sits at, above its centre's ground. The
/// ground at a footprint's near corner is `side` deep: nearer than the sprite
/// when the side is over this, so it cut the sprite's lowest rows along a
/// horizontal line (the castle's front corner).
pub(in crate::game_renderer) const BUILDING_CLEARANCE: f64 = 3.0;
/// Slack so the corner triangles' own vertices stay behind the sprite.
const MARGIN: f64 = 0.5;

/// A footprint whose ground is deeper than its sprite.
pub(super) struct Pit {
    center: [f64; 2],
    half: f64,
    /// Elevation removed from the ground's depth; one metre is two depth units.
    sink: f64,
}

impl Pit {
    pub(super) fn for_building(unit: SceneUnit) -> Option<Self> {
        let side = unit.kind.building_side()?;
        let deck = unit
            .kind
            .building_slot()
            .is_some_and(|slot| (65..=70).contains(&slot));
        (side > BUILDING_CLEARANCE && !deck && !unit.visual.ghost()).then(|| Self {
            center: unit.position,
            half: side / 2.0,
            sink: (side - BUILDING_CLEARANCE + MARGIN) / 2.0,
        })
    }

    pub(super) fn reach(&self) -> f64 {
        self.half
    }

    pub(super) fn position(&self) -> [f64; 2] {
        self.center
    }

    /// Lower the depth of a face under the footprint; its screen points stay.
    pub(super) fn sink(&self, face: &mut ProjectedSurfaceTriangle) {
        let points = &mut face.points;
        let at = [0, 1].map(|axis| {
            (points[0].world[axis] + points[1].world[axis] + points[2].world[axis]) / 3.0
        });
        if (0..2).all(|axis| (at[axis] - self.center[axis]).abs() < self.half) {
            for point in points.iter_mut() {
                point.world[2] -= self.sink;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{test_camera, test_unit};
    use super::*;
    use wasm_bindgen_test::wasm_bindgen_test;

    #[wasm_bindgen_test]
    fn big_buildings_sink_the_ground_under_their_footprint_behind_their_sprite() {
        // A flat sprite at the centre depth plus the clearance is shallower than the
        // ground under the near corner of a four-tile footprint; that ground cut the
        // castle's lowest rows along a horizontal line.
        let camera = test_camera();
        let castle = SceneUnit {
            position: [2.0, 2.0],
            ..test_unit(SceneUnitKind::Castle)
        };
        let tiles = (0..6)
            .flat_map(|y| (0..6).map(move |x| (x, y)))
            .map(|(x, y)| SceneTerrain {
                position: [f64::from(x) + 0.5, f64::from(y) + 0.5],
                material: 0,
                elevation_meters: 0.0,
                surface: SceneTerrainSurface::flat(0.0),
                appearance: None,
            })
            .collect::<Vec<_>>();
        let faces = projected_surface_triangles(&tiles, camera);
        let ground = BuildingGround::new(&[castle], camera);
        let sprite = scene_unit_depth(castle);
        let deepest = |face: &ProjectedSurfaceTriangle| {
            face.points
                .iter()
                .map(|p| surface_render_depth(p.world, face.skirt))
                .fold(f64::NEG_INFINITY, f64::max)
        };
        let (mut under, mut beside) = (0, 0);
        for original in faces {
            let mut face = original;
            ground.apply(&mut face);
            let at = |face: &ProjectedSurfaceTriangle, axis: usize| {
                face.points.iter().map(|p| p.world[axis]).sum::<f64>() / 3.0
            };
            let inside = (0..2).all(|axis| (at(&original, axis) - 2.0).abs() < 2.0);
            if inside {
                under += 1;
                assert!(deepest(&face) <= sprite, "ground in front of the sprite");
            } else {
                beside += 1;
                assert_eq!(deepest(&face), deepest(&original), "outside is untouched");
            }
            for (after, before) in face.points.iter().zip(original.points) {
                assert_eq!(after.screen, before.screen, "only depth moves");
            }
        }
        assert!(under >= 32 && beside > 0);
        // Sized to its footprint: a house keeps its ground.
        let house = SceneUnit {
            position: [2.0, 2.0],
            ..test_unit(SceneUnitKind::House)
        };
        assert!(BuildingGround::new(&[house], camera).pits.is_empty());
    }
}

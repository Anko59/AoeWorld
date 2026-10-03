//! One projected terrain snapshot shared by rendering, contacts and picking.
use super::*;
use aoe_rendering::{ProjectedSurfaceTriangle, SceneCamera, WorldKeyIndex};

pub(crate) struct PreparedScene {
    camera: Camera,
    pub terrain: Vec<SceneTerrain>,
    pub triangles: Vec<ProjectedSurfaceTriangle>,
    bucket_keys: WorldKeyIndex,
    buckets: Vec<Vec<usize>>,
    bucket_width: f64,
}

pub(in super::super) fn prepare(client: &Client) -> Rc<PreparedScene> {
    if let Some(scene) = client.terrain_scene.borrow().as_ref()
        && scene.camera == client.camera
    {
        return scene.clone();
    }
    let scene = Rc::new(PreparedScene::new(client.camera, scene_terrain(client)));
    *client.terrain_scene.borrow_mut() = Some(scene.clone());
    scene
}

impl PreparedScene {
    fn new(camera: Camera, terrain: Vec<SceneTerrain>) -> Self {
        let triangles = aoe_rendering::projected_surface_triangles(
            &terrain,
            SceneCamera {
                center: camera.center,
                zoom: camera.zoom,
                viewport: camera.viewport,
                focus_elevation_meters: camera.focus_elevation_meters,
            },
        );
        let bucket_width = triangles
            .iter()
            .filter(|triangle| !triangle.skirt)
            .map(|triangle| {
                let (min, max) = bounds(triangle);
                (max[0] - min[0]).max(max[1] - min[1])
            })
            .fold(8.0_f64, f64::max);
        let mut bucket_keys = WorldKeyIndex::default();
        let mut buckets = Vec::<Vec<usize>>::new();
        for (index, triangle) in triangles.iter().enumerate().filter(|(_, t)| !t.skirt) {
            let (min, max) = bounds(triangle);
            for y in bucket(min[1], bucket_width)..=bucket(max[1], bucket_width) {
                for x in bucket(min[0], bucket_width)..=bucket(max[0], bucket_width) {
                    let slot = bucket_keys.entry([x, y]);
                    if slot == buckets.len() {
                        buckets.push(Vec::new());
                    }
                    buckets[slot].push(index);
                }
            }
        }
        // World XY corners are integral, and bucket width covers each triangle
        // on both axes: at most four touched buckets per displayed triangle.
        debug_assert!(
            buckets.iter().map(Vec::len).sum::<usize>() <= triangles.len().saturating_mul(4)
        );
        Self {
            camera,
            terrain,
            triangles,
            bucket_keys,
            buckets,
            bucket_width,
        }
    }

    /// Query the actual displayed top surface, including its current LOD.
    /// Spatial buckets avoid scanning the full mesh for every tree or unit.
    pub(crate) fn height(&self, world: [f64; 2]) -> Option<f64> {
        let key = [
            bucket(world[0], self.bucket_width),
            bucket(world[1], self.bucket_width),
        ];
        self.buckets
            .get(self.bucket_keys.get(key)?)?
            .iter()
            .find_map(|&index| {
                let [a, b, c] = self.triangles[index].points.map(|p| p.world);
                let denominator = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
                if denominator.abs() < f64::EPSILON {
                    return None;
                }
                let first = ((b[1] - c[1]) * (world[0] - c[0]) + (c[0] - b[0]) * (world[1] - c[1]))
                    / denominator;
                let second = ((c[1] - a[1]) * (world[0] - c[0])
                    + (a[0] - c[0]) * (world[1] - c[1]))
                    / denominator;
                let third = 1.0 - first - second;
                (first >= -1e-7 && second >= -1e-7 && third >= -1e-7)
                    .then_some(first * a[2] + second * b[2] + third * c[2])
            })
    }
}

fn bucket(value: f64, width: f64) -> i32 {
    (value / width).floor() as i32
}
fn bounds(triangle: &ProjectedSurfaceTriangle) -> ([f64; 2], [f64; 2]) {
    triangle.points.iter().fold(
        ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]),
        |(mut min, mut max), p| {
            for axis in 0..2 {
                min[axis] = min[axis].min(p.world[axis]);
                max[axis] = max[axis].max(p.world[axis]);
            }
            (min, max)
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::wasm_bindgen_test;

    #[wasm_bindgen_test]
    fn unique_contact_keys_preserve_reference_bucket_triangle_order() {
        let terrain = (0..64)
            .flat_map(|y| {
                (0..64).map(move |x| SceneTerrain {
                    position: [f64::from(x) + 0.5, f64::from(y) + 0.5],
                    material: 0,
                    elevation_meters: f64::from((x % 4) * 2),
                    surface: SceneTerrainSurface::flat(f64::from((x % 4) * 2)),
                })
            })
            .collect();
        let camera = Camera {
            center: [32.0, 32.0],
            zoom: 0.25,
            viewport: [1280.0, 720.0],
            focus_elevation_meters: 0.0,
        };
        let scene = PreparedScene::new(camera, terrain);
        let mut expected = std::collections::BTreeMap::<_, Vec<_>>::new();
        for (index, triangle) in scene.triangles.iter().enumerate().filter(|(_, t)| !t.skirt) {
            let (min, max) = bounds(triangle);
            for y in bucket(min[1], scene.bucket_width)..=bucket(max[1], scene.bucket_width) {
                for x in bucket(min[0], scene.bucket_width)..=bucket(max[0], scene.bucket_width) {
                    expected.entry([x, y]).or_default().push(index);
                }
            }
        }
        assert_eq!(scene.buckets.len(), expected.len());
        assert!(scene.buckets.len() <= scene.triangles.len() * 4);
        assert!(scene.buckets.iter().map(Vec::len).sum::<usize>() <= scene.triangles.len() * 4);
        for (key, indexes) in expected {
            let slot = scene.bucket_keys.get(key).unwrap();
            assert_eq!(scene.buckets[slot], indexes);
        }
    }

    #[wasm_bindgen_test]
    fn contacts_follow_projected_coarse_triangles_instead_of_fine_height() {
        let terrain = (0..128)
            .flat_map(|y| {
                (0..128).map(move |x| SceneTerrain {
                    position: [f64::from(x) + 0.5, f64::from(y) + 0.5],
                    material: 0,
                    elevation_meters: f64::from((x % 4) * 2),
                    surface: SceneTerrainSurface {
                        corner_game_height_levels: [(x % 4 * 2) as i16; 4],
                        kind: 0,
                        triangulation: 0,
                        water: 0,
                    },
                })
            })
            .collect();
        let camera = Camera {
            center: [64.0, 64.0],
            zoom: 0.25,
            viewport: [1280.0, 720.0],
            focus_elevation_meters: 0.0,
        };
        let scene = PreparedScene::new(camera, terrain);
        let mut checked = 0;
        let mut changed = false;
        for triangle in scene.triangles.iter().filter(|t| !t.skirt) {
            let world =
                [0, 1].map(|axis| triangle.points.iter().map(|p| p.world[axis]).sum::<f64>() / 3.0);
            let expected = triangle.points.iter().map(|p| p.world[2]).sum::<f64>() / 3.0;
            assert!((scene.height(world).unwrap() - expected).abs() < 1e-6);
            changed |= (expected - (world[0].floor() % 4.0) * 2.0).abs() > 0.1;
            checked += 1;
        }
        assert!(checked > 100);
        assert!(
            changed,
            "fixture must exercise fine-versus-coarse height disagreement"
        );
        assert!(scene.height([-20.0, -20.0]).is_none());
    }
}

pub(in super::super) fn units(
    client: &Client,
    scene: &PreparedScene,
) -> Vec<aoe_rendering::SceneUnit> {
    client
        .units
        .values()
        .map(|unit| {
            let position = client.presentation.position(*unit);
            aoe_rendering::SceneUnit {
                id: unit.id,
                position,
                moving: unit.moving || position != unit.position.as_tiles(),
                facing: unit.facing,
                selected: client.selected == Some(unit.id),
                elevation_meters: scene
                    .height(position)
                    .unwrap_or_else(|| elevation_at_world(client, position)),
            }
        })
        .collect()
}

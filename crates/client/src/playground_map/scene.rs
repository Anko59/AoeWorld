//! One projected terrain snapshot shared by rendering, contacts and picking.
use super::*;
use aoe_rendering::{ProjectedSurfaceTriangle, SceneCamera, WorldKeyIndex};

#[cfg(test)]
#[path = "scene/tests/review.rs"]
mod review_tests;
#[cfg(test)]
#[path = "scene/tests/worker.rs"]
mod worker_tests;

pub(crate) struct PreparedScene {
    camera: Camera,
    pub terrain: Vec<SceneTerrain>,
    pub decorations: Vec<SceneDecoration>,
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
    let mut scene = PreparedScene::new(client.camera, scene_terrain(client));
    scene.decorations = decorations::for_scene(client, &scene);
    let scene = Rc::new(scene);
    *client.terrain_scene.borrow_mut() = Some(scene.clone());
    scene
}

impl PreparedScene {
    pub(super) fn new(camera: Camera, terrain: Vec<SceneTerrain>) -> Self {
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
            decorations: Vec::new(),
            triangles,
            bucket_keys,
            buckets,
            bucket_width,
        }
    }

    /// Query the ground contact surface, including its current LOD. Cosmetic
    /// cliff rims never change eligible ground's source contact coordinates.
    /// Spatial buckets avoid scanning the full mesh for every tree or unit.
    pub(crate) fn height(&self, world: [f64; 2]) -> Option<f64> {
        let key = [
            bucket(world[0], self.bucket_width),
            bucket(world[1], self.bucket_width),
        ];
        let indices = self.buckets.get(self.bucket_keys.get(key)?)?;
        if let Some(height) = indices.iter().find_map(|&index| {
            triangle_height(self.triangles[index].source_ground_points()?, world)
        }) {
            return Some(height);
        }
        indices.iter().find_map(|&index| {
            triangle_height(self.triangles[index].points.map(|p| p.world), world)
        })
    }
}

fn triangle_height(points: [[f64; 3]; 3], world: [f64; 2]) -> Option<f64> {
    let [a, b, c] = points;
    let denominator = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
    if denominator.abs() < f64::EPSILON {
        return None;
    }
    let first =
        ((b[1] - c[1]) * (world[0] - c[0]) + (c[0] - b[0]) * (world[1] - c[1])) / denominator;
    let second =
        ((c[1] - a[1]) * (world[0] - c[0]) + (a[0] - c[0]) * (world[1] - c[1])) / denominator;
    let third = 1.0 - first - second;
    (first >= -1e-7 && second >= -1e-7 && third >= -1e-7)
        .then_some(first * a[2] + second * b[2] + third * c[2])
}

fn bucket(value: f64, width: f64) -> i32 {
    (value / width).floor() as i32
}
fn bounds(triangle: &ProjectedSurfaceTriangle) -> ([f64; 2], [f64; 2]) {
    triangle
        .points
        .iter()
        .map(|p| p.world)
        .chain(triangle.source_ground_points().into_iter().flatten())
        .fold(
            ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]),
            |(mut min, mut max), p| {
                for axis in 0..2 {
                    min[axis] = min[axis].min(p[axis]);
                    max[axis] = max[axis].max(p[axis]);
                }
                (min, max)
            },
        )
}

#[cfg(test)]
#[path = "scene/tests/contacts.rs"]
mod tests;

pub(in super::super) fn units(
    client: &Client,
    scene: &PreparedScene,
) -> Vec<aoe_rendering::SceneUnit> {
    let mut units = client
        .units
        .values()
        .map(|unit| {
            let position = client.presentation.position(*unit);
            aoe_rendering::SceneUnit {
                kind: worker_kind(unit),
                id: unit.id,
                position,
                moving: unit.moving || position != unit.position.as_tiles(),
                facing: unit.facing,
                selected: client.selected.units.contains(&unit.id),
                elevation_meters: scene
                    .height(position)
                    .unwrap_or_else(|| elevation_at_world(client, position)),
            }
        })
        .collect::<Vec<_>>();
    for building in &client.village.buildings {
        let position = super::super::village::building_center(building);
        let slot = units.partition_point(|unit| unit.id < building.id);
        units.insert(
            slot,
            aoe_rendering::SceneUnit {
                kind: aoe_rendering::SceneUnitKind::TownCenter,
                id: building.id,
                position,
                moving: false,
                facing: 0,
                selected: client.selected.building_id == Some(building.id),
                elevation_meters: scene
                    .height(position)
                    .unwrap_or_else(|| elevation_at_world(client, position)),
            },
        );
    }
    units
}

fn worker_kind(unit: &aoe_protocol::GameplayUnitState) -> aoe_rendering::SceneUnitKind {
    use aoe_protocol::{ResourceKind, UnitKind, UnitTask};
    use aoe_rendering::SceneUnitKind as Art;
    if unit.kind == UnitKind::Scout {
        return Art::Scout;
    }
    match (unit.task, unit.gathering, unit.carried) {
        // The catalog has no farmer art: farming draws the foraging pose.
        (UnitTask::Farming, _, _) => Art::VillagerForaging,
        (UnitTask::Gathering, Some(kind), _) => match kind {
            ResourceKind::Food => Art::VillagerForaging,
            ResourceKind::Wood => Art::VillagerChopping,
            ResourceKind::Gold => Art::VillagerMiningGold,
            ResourceKind::Stone => Art::VillagerMiningStone,
        },
        (UnitTask::Returning, _, Some(load)) if load.amount > 0 => match load.kind {
            ResourceKind::Food => Art::VillagerCarryingFood,
            ResourceKind::Wood => Art::VillagerCarryingWood,
            ResourceKind::Gold => Art::VillagerCarryingGold,
            ResourceKind::Stone => Art::VillagerCarryingStone,
        },
        _ => Art::Villager,
    }
}

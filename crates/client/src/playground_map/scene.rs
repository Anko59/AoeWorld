//! One projected terrain snapshot shared by rendering, contacts and picking.
use super::*;
use aoe_rendering::{ProjectedSurfaceTriangle, SceneCamera, WorldKeyIndex};
#[path = "scene/actors.rs"]
mod actors;
#[path = "scene/bridges.rs"]
mod bridges;
#[path = "scene/projectiles.rs"]
mod projectiles;

#[cfg(test)]
#[path = "scene/tests/cache.rs"]
mod cache_tests;
#[cfg(test)]
#[path = "scene/tests/review.rs"]
mod review_tests;
#[cfg(test)]
#[path = "scene/tests/worker.rs"]
mod worker_tests;

pub(crate) struct PreparedScene {
    camera: Camera,
    foundations: Vec<(aoe_rendering::SceneUnitKind, [f64; 2])>,
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
    if super::super::strategic::active(client) {
        let scene = Rc::new(PreparedScene::new(client.camera, Vec::new()));
        *client.terrain_scene.borrow_mut() = Some(scene.clone());
        return scene;
    }
    let foundations = client
        .village
        .buildings
        .iter()
        .map(|b| (building_kind(b), super::super::village::building_center(b)))
        .collect::<Vec<_>>();
    let mut terrain = scene_terrain(client);
    let units = client
        .village
        .buildings
        .iter()
        .map(|building| aoe_rendering::SceneUnit {
            kind: building_kind(building),
            id: building.id,
            position: super::super::village::building_center(building),
            visual: aoe_rendering::SceneUnitVisual::new(false).with_selected(false),
            facing: 0,
            elevation_meters: 0.0,
        })
        .collect::<Vec<_>>();
    aoe_rendering::apply_building_foundations(&mut terrain, &client.art, &units);
    let mut scene = PreparedScene::new(client.camera, terrain);
    scene.foundations = foundations;
    scene.decorations = decorations::for_scene(client, &scene);
    let scene = Rc::new(scene);
    *client.terrain_scene.borrow_mut() = Some(scene.clone());
    scene
}

/// Building updates are the only foundation mutations. Compare the cached key
/// here once per update, without allocating it again for each ground projection.
pub(in super::super) fn replace_buildings(
    client: &mut Client,
    buildings: Vec<aoe_protocol::GameplayBuildingState>,
) {
    let changed = client
        .terrain_scene
        .get_mut()
        .as_ref()
        .is_some_and(|scene| {
            !scene.foundations.iter().copied().eq(buildings
                .iter()
                .map(|b| (building_kind(b), super::super::village::building_center(b))))
        });
    if changed {
        client.terrain_scene.get_mut().take();
    }
    client.village.buildings = buildings;
}

impl PreparedScene {
    pub(crate) fn resident_bytes(&self) -> usize {
        self.bucket_keys.resident_bytes()
            + self.terrain.capacity() * size_of::<SceneTerrain>()
            + self.decorations.capacity() * size_of::<SceneDecoration>()
            + self.triangles.capacity() * size_of::<ProjectedSurfaceTriangle>()
            + self.foundations.capacity() * size_of::<(aoe_rendering::SceneUnitKind, [f64; 2])>()
            + self.buckets.capacity() * size_of::<Vec<usize>>()
            + self
                .buckets
                .iter()
                .map(|b| b.capacity() * size_of::<usize>())
                .sum::<usize>()
    }

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
            foundations: Vec::new(),
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

/// Subunits from an attacker to its target's centre, or to the edge of a
/// building's footprint; `None` while the client does not know the target.
fn target_distance(client: &Client, unit: &aoe_protocol::GameplayUnitState) -> Option<i64> {
    let target = unit.attack_target?;
    let (x, y) = (i64::from(unit.position.x), i64::from(unit.position.y));
    let (tx, ty) = if let Some(other) = client.units.get(&target) {
        (i64::from(other.position.x), i64::from(other.position.y))
    } else {
        let footprint = client
            .village
            .buildings
            .iter()
            .find(|b| b.id == target)?
            .footprint;
        let tile = i64::from(aoe_core::FIXED_SUBUNITS_PER_TILE);
        (
            x.clamp(
                i64::from(footprint.min.x) * tile,
                i64::from(footprint.max.x) * tile,
            ),
            y.clamp(
                i64::from(footprint.min.y) * tile,
                i64::from(footprint.max.y) * tile,
            ),
        )
    };
    Some(((x - tx).pow(2) + (y - ty).pow(2)).isqrt())
}

pub(in super::super) fn units(
    client: &Client,
    scene: &PreparedScene,
) -> Vec<aoe_rendering::SceneUnit> {
    let player = super::super::village::owner(client);
    let mut units = client
        .units
        .values()
        .filter(|u| {
            u.player == player
                || client
                    .fog
                    .state([u.position.tile_floor().x, u.position.tile_floor().y])
                    == 2
        })
        .map(|unit| {
            let position = client.presentation.position(*unit);
            let animal = unit
                .hunt_target
                .and_then(|id| client.village.animals.iter().find(|a| a.id == id));
            let pose = if crate::unit_visuals::walks_as_villager(unit, animal) {
                crate::unit_visuals::Pose::VillagerWalk
            } else {
                crate::unit_visuals::lifecycle_pose(
                    crate::unit_visuals::scene_pose(unit, animal, target_distance(client, unit)),
                    unit.death_tick,
                    client.village.tick,
                    client.config.tick_hz,
                )
            };
            let kind = aoe_rendering::SceneUnitKind::actor(pose as u8);
            let facing = actors::animation_frame(client, unit, pose);
            aoe_rendering::SceneUnit {
                kind,
                id: unit.id,
                position,
                visual: aoe_rendering::SceneUnitVisual::new(
                    unit.moving || position != unit.position.as_tiles(),
                )
                .with_player(Some(unit.player), player, client.team_colour, unit.hp > 0)
                .with_selected(unit.hp > 0 && client.selected.units.contains(&unit.id)),
                facing: 0,
                elevation_meters: scene
                    .height(position)
                    .unwrap_or_else(|| elevation_at_world(client, position)),
            }
            .with_actor_frame(facing)
        })
        .collect::<Vec<_>>();
    for animal in &client.village.animals {
        let position = animal.position.as_tiles();
        let pose = crate::unit_visuals::lifecycle_pose(
            crate::unit_visuals::animal_pose(animal),
            animal.death_tick,
            client.village.tick.max(client.village.wildlife_tick),
            client.config.tick_hz,
        );
        let frame = crate::unit_visuals::packed_frame(
            pose,
            animal.facing,
            client.last_frame,
            animal.death_tick,
            client
                .presentation
                .live_tick(client.last_frame)
                .unwrap_or(client.village.tick as f64)
                .max(client.village.wildlife_tick as f64),
            client.config.tick_hz,
        );
        let unit = aoe_rendering::SceneUnit {
            kind: aoe_rendering::SceneUnitKind::actor(pose as u8),
            id: animal.id,
            position,
            visual: aoe_rendering::SceneUnitVisual::new(animal.moving)
                .with_player(animal.owner, player, client.team_colour, animal.hp > 0)
                .with_selected(client.selected.units.contains(&animal.id) && animal.hp > 0),
            facing: 0,
            elevation_meters: scene
                .height(position)
                .unwrap_or_else(|| elevation_at_world(client, position)),
        }
        .with_actor_frame(frame);
        let slot = units.partition_point(|u| u.id < animal.id);
        units.insert(slot, unit);
    }
    let walls: Vec<_> = client
        .village
        .buildings
        .iter()
        .filter(|b| b.kind.is_wall())
        .map(|b| (b.footprint.min.x, b.footprint.min.y, b.kind))
        .collect();
    for building in &client.village.buildings {
        if building.kind == aoe_protocol::BuildingKind::Bridge {
            bridges::append(client, scene, building, &mut units);
            continue;
        }
        let position = super::super::village::building_center(building);
        let kind = crate::building_ui::gate_kind(
            building.kind,
            building.gate_open,
            building.progress_permille,
        )
        .unwrap_or_else(|| building_kind(building));
        let stage = client
            .art
            .compositions
            .iter()
            .find(|b| Some(b.unit_id) == kind.building_id())
            .map(|art| art.damage_stage(building.hp, building.max_hp))
            .unwrap_or(0);
        let unit = aoe_rendering::SceneUnit {
            kind,
            id: building.id,
            position,
            visual: aoe_rendering::SceneUnitVisual::new(false)
                .with_player(Some(building.player), player, client.team_colour, false)
                .with_selected(client.selected.has_building(building.id))
                .with_damage_stage(stage),
            facing: if building.progress_permille < 1000 {
                (u32::from(building.progress_permille) * 3 / 1000) as u8
            } else if building.kind.is_wall() {
                crate::building_ui::wall_frame(building.footprint.min, |t| {
                    walls.contains(&(t.x, t.y, building.kind))
                })
            } else {
                0
            },
            elevation_meters: scene
                .height(position)
                .unwrap_or_else(|| elevation_at_world(client, position)),
        };
        if building.kind.is_gate() {
            if !aoe_rendering::scene_building_visible(
                &client.art,
                unit,
                SceneCamera {
                    center: client.camera.center,
                    zoom: client.camera.zoom,
                    viewport: client.camera.viewport,
                    focus_elevation_meters: client.camera.focus_elevation_meters,
                },
                0,
            ) {
                continue;
            }
            client.village.gate_art.borrow_mut().observe(
                building.id,
                building.gate_open,
                building.progress_permille,
                true,
            );
        }
        let slot = units.partition_point(|unit| unit.id < building.id);
        units.insert(slot, unit);
    }
    for (building, expiry) in &client.village.rubble {
        if *expiry <= client.village.tick || building.kind == aoe_protocol::BuildingKind::Bridge {
            continue;
        }
        let position = super::super::village::building_center(building);
        let unit = aoe_rendering::SceneUnit {
            kind: aoe_rendering::SceneUnitKind::building(crate::building_ui::age_sprite_slot(
                building.kind,
                1000,
                false,
                building.age,
            ) as u8),
            id: building.id,
            position,
            visual: aoe_rendering::SceneUnitVisual::new(true)
                .with_player(Some(building.player), player, client.team_colour, false)
                .with_selected(false),
            facing: 0,
            elevation_meters: scene
                .height(position)
                .unwrap_or_else(|| elevation_at_world(client, position)),
        };
        let slot = units.partition_point(|u| u.id < building.id);
        units.insert(slot, unit);
    }
    projectiles::append(client, scene, &mut units);
    super::super::village::building_panel::preview(client, &mut units);
    units
}

fn building_kind(building: &aoe_protocol::GameplayBuildingState) -> aoe_rendering::SceneUnitKind {
    #[cfg(test)]
    cache_tests::KEY_ENTRIES.with(|count| count.set(count.get() + 1));
    use aoe_protocol::{BuildingKind as Kind, ConstructionState};
    use aoe_rendering::SceneUnitKind as Art;
    let kind = if building.kind == Kind::TownCenter && building.age == aoe_protocol::Age::Dark {
        Art::TownCenter
    } else if building.age == aoe_protocol::Age::Feudal && building.kind == Kind::LumberCamp {
        Art::FeudalLumberCamp
    } else if building.age == aoe_protocol::Age::Feudal && building.kind == Kind::MiningCamp {
        Art::FeudalMiningCamp
    } else {
        Art::building(
            crate::building_ui::age_sprite_slot(building.kind, 1000, false, building.age) as u8,
        )
    };
    if building.construction == ConstructionState::Complete {
        kind
    } else if let Some(art) =
        crate::building_ui::stone_foundation(building.kind, building.progress_permille)
    {
        art
    } else {
        kind.building_id()
            .map_or(kind, |id| Art::foundation(id as i16))
    }
}

/// Full native destruction followed by three seconds of its DAT corpse art.
pub(in super::super) fn rubble_ticks(
    client: &Client,
    building: &aoe_protocol::GameplayBuildingState,
) -> u64 {
    let kind = aoe_rendering::SceneUnitKind::building(crate::building_ui::age_sprite_slot(
        building.kind,
        1000,
        false,
        building.age,
    ) as u8);
    let millis = client
        .art
        .compositions
        .iter()
        .find(|b| Some(b.unit_id) == kind.building_id())
        .map_or(0, |b| b.destruction_millis());
    (u64::from(millis) + 3000)
        .saturating_mul(u64::from(client.config.tick_hz))
        .div_ceil(1000)
}

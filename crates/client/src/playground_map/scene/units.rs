//! Viewport actor preparation; authoritative world storage remains complete.
use super::*;

fn visible(
    client: &Client,
    scene: &PreparedScene,
    position: [f64; 2],
    height: f32,
    building: bool,
) -> bool {
    scene.actor_extents.is_none_or(|extents| {
        crate::scene_culling::visible(
            client.camera,
            position,
            f64::from(height),
            extents[usize::from(building)],
        )
    })
}

pub(in super::super::super) fn units(
    client: &Client,
    scene: &PreparedScene,
) -> Vec<aoe_rendering::SceneUnit> {
    let player = super::super::super::village::owner(client);
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
        .filter_map(|unit| {
            let (position, travelled) = client.presentation.pose(*unit);
            let height = scene
                .height(position)
                .unwrap_or_else(|| elevation_at_world(client, position))
                as f32;
            if !visible(client, scene, position, height, false) {
                return None;
            }
            let (shown, pose) = actors::shown_pose(client, unit);
            let unit = &shown;
            let kind = aoe_rendering::SceneUnitKind::actor(pose as u8);
            let facing = actors::animation_frame(client, unit, pose, travelled);
            Some(
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
                    elevation_meters: height,
                }
                .with_actor_frame(facing),
            )
        })
        .collect::<Vec<_>>();
    for animal in &client.village.animals {
        let position = animal.position.as_tiles();
        let height = scene
            .height(position)
            .unwrap_or_else(|| elevation_at_world(client, position)) as f32;
        if !visible(client, scene, position, height, false) {
            continue;
        }
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
            elevation_meters: height,
        }
        .with_actor_frame(frame);
        units.push(unit);
    }
    // Index wall families once instead of scanning every wall for each neighbor.
    let walls: std::collections::HashSet<_> = client
        .village
        .buildings
        .iter()
        .filter(|b| b.kind.is_wall())
        .map(|b| (b.footprint.min.x, b.footprint.min.y, b.kind as u8))
        .collect();
    for building in &client.village.buildings {
        if building.kind == aoe_protocol::BuildingKind::Road {
            continue;
        }
        if building.kind == aoe_protocol::BuildingKind::Bridge {
            bridges::append(client, scene, building, &mut units);
            continue;
        }
        let position = super::super::super::village::building_center(building);
        let height = scene
            .height(position)
            .unwrap_or_else(|| elevation_at_world(client, position)) as f32;
        if !visible(client, scene, position, height, true) {
            continue;
        }
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
                    walls.contains(&(t.x, t.y, building.kind as u8))
                })
            } else {
                0
            },
            elevation_meters: height,
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
        units.push(unit);
    }
    for (building, expiry) in &client.village.rubble {
        if *expiry <= client.village.tick || building.kind == aoe_protocol::BuildingKind::Bridge {
            continue;
        }
        let position = super::super::super::village::building_center(building);
        let height = scene
            .height(position)
            .unwrap_or_else(|| elevation_at_world(client, position)) as f32;
        if !visible(client, scene, position, height, true) {
            continue;
        }
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
            elevation_meters: height,
        };
        units.push(unit);
    }
    // One stable sort avoids shifting the growing actor tail for every
    // building. Equal-ID bridge pieces retain their native span order.
    units.sort_by_key(|unit| unit.id);
    projectiles::append(client, scene, &mut units);
    super::super::super::village::building_panel::preview(client, &mut units);
    units
}

use super::*;
use aoe_core::EntityId;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn fish_sprites_animate_at_their_sampled_native_period() {
    let mut art = art();
    let fish = SceneResource {
        id: 0,
        position: [0.5; 2],
        kind: 0,
        visual_variant: 0,
        visual_family: 5,
        elevation_meters: 0.0,
    };
    art.fishing[0] = art.village[1][..17].to_vec();
    art.fishing[1] = art.village[1][..7].to_vec();
    for (family, period) in [(5, 3), (6, 9)] {
        let resource = SceneResource {
            visual_family: family,
            ..fish
        };
        let frame_at = |clock| {
            world_sprite_frames(&art, &[], &[resource], &[], camera(), clock)
                .last()
                .unwrap()
                .1
                .atlas
        };
        assert_eq!(frame_at(0), frame_at(period - 1));
        assert_ne!(frame_at(0), frame_at(period));
    }
}

#[wasm_bindgen_test]
fn fishing_poses_use_native_facing_rows() {
    let mut art = art();
    art.fishing[7] = vec![art.walking[0]; 5];
    art.fishing[10] = art.village[1][..35].to_vec();
    let ship = unit(SceneUnitKind::FishingShipWorking);
    assert_eq!(frames(&art, ship).unwrap().1, 1);
    let fisher = unit(SceneUnitKind::VillagerFishing);
    assert_eq!(frames(&art, fisher).unwrap().1, 7);
    let frame_at = |clock| {
        world_sprite_frames(&art, &[], &[], &[fisher], camera(), clock)
            .last()
            .unwrap()
            .1
            .atlas
    };
    assert_eq!(
        frame_at(0),
        frame_at(7),
        "four skipped 200 ms native frames take 800 ms"
    );
    assert_ne!(frame_at(0), frame_at(8));
}

fn art() -> GameArt {
    let frame = |number| GameFrame {
        atlas: crate::AtlasAddress {
            page: 2,
            uv: [number as f32 / 4096.0, 0.0, 0.01, 0.01],
        },
        size: [24.0, 45.0],
        anchor: [12.0, 40.0],
    };
    GameArt {
        actors: Vec::new(),
        buildings: Default::default(),
        village: std::array::from_fn(|group| {
            (0..75)
                .step_by(if group == 0 { 15 } else { 1 })
                .map(|index| frame(group * 100 + index))
                .collect()
        }),
        fishing: Default::default(),
        town_center: std::array::from_fn(|group| vec![frame(500 + group)]),
        walking: vec![frame(900); 50],
        standing: vec![frame(901); 5],
        grass: Vec::new(),
        terrain: Default::default(),
        terrain_topology: [None; 12],
        water_depth: Default::default(),
        resources: Default::default(),
        tree_shadows: Vec::new(),
        tree_families: Default::default(),
        scenery: Default::default(),
    }
}
fn camera() -> SceneCamera {
    SceneCamera {
        center: [0.5; 2],
        zoom: 1.0,
        viewport: [800.0, 600.0],
        focus_elevation_meters: 0.0,
    }
}
fn unit(kind: SceneUnitKind) -> SceneUnit {
    SceneUnit {
        kind,
        id: EntityId(2),
        position: [0.5; 2],
        moving: false,
        facing: 0,
        selected: false,
        elevation_meters: 0.0,
    }
}

#[wasm_bindgen_test]
fn actor_picking_bounds_follow_the_native_body_and_mirrored_hotspot() {
    let mut art = art();
    art.actors = vec![vec![
        GameFrame {
            atlas: crate::AtlasAddress::WHITE,
            size: [12.0, 18.0],
            anchor: [2.0, 16.0]
        };
        5
    ]];
    let mut u = unit(SceneUnitKind::actor(0));
    u.position = [0.0, 0.0];
    let mut camera = camera();
    camera.center = [0.0, 0.0];
    let bounds = super::super::scene_unit_bounds(&art, u, camera, 0).unwrap();
    let foot = camera.viewport[0] / 2.0;
    assert!((bounds[0] - (foot - 2.0 * 4.0 / 3.0)).abs() < 0.001);
    u.facing = 2;
    let bounds = super::super::scene_unit_bounds(&art, u, camera, 0).unwrap();
    assert!((bounds[0] - (foot - 10.0 * 4.0 / 3.0)).abs() < 0.001);
}

#[wasm_bindgen_test]
fn villager_walk_uses_fifteen_frame_rows_for_all_eight_facings() {
    let art = art();
    for (facing, index) in [107, 122, 137, 152, 167, 152, 137, 122]
        .into_iter()
        .enumerate()
    {
        let mut u = unit(SceneUnitKind::Villager);
        u.facing = facing as u8;
        u.moving = true;
        let frames = super::super::world_sprite_frames(&art, &[], &[], &[u], camera(), 7);
        assert_eq!(frames.last().unwrap().1.atlas.uv[0], index as f32 / 4096.0);
        assert_eq!(
            frames.last().unwrap().0.uv[2] < 0.0,
            matches!(facing, 1..=3)
        );
    }
}
#[wasm_bindgen_test]
fn working_and_carrying_use_reviewed_art_and_absence_draws_a_marker() {
    let mut art = art();
    for (kind, expected) in [
        (SceneUnitKind::VillagerChopping, 207),
        (SceneUnitKind::VillagerCarryingWood, 300),
    ] {
        let frames = super::super::world_sprite_frames(&art, &[], &[], &[unit(kind)], camera(), 7);
        assert_eq!(
            frames.last().unwrap().1.atlas.uv[0],
            expected as f32 / 4096.0
        );
    }
    art.village = Default::default();
    let frames = super::super::world_sprite_frames(
        &art,
        &[],
        &[],
        &[unit(SceneUnitKind::Villager)],
        camera(),
        7,
    );
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].0.color, [0.25, 0.57, 0.96, 1.0]);
}
#[wasm_bindgen_test]
fn town_center_composes_eight_native_parts_or_one_fallback_marker() {
    let mut art = art();
    let u = unit(SceneUnitKind::TownCenter);
    let frames = super::super::world_sprite_frames(&art, &[], &[], &[u], camera(), 0);
    assert_eq!(frames.len(), 8);
    assert_eq!(frames[0].1.atlas.uv[0], 507.0 / 4096.0);
    let main = frames
        .iter()
        .find(|f| f.1.atlas.uv[0] == 500.0 / 4096.0)
        .unwrap();
    for (source, offset) in [(501.0, 48.0), (504.0, 24.0)] {
        let roof = frames
            .iter()
            .find(|f| f.1.atlas.uv[0] == source / 4096.0)
            .unwrap();
        assert!((main.1.anchor[1] - roof.1.anchor[1] - offset * 4.0 / 3.0).abs() < 0.001);
    }
    art.town_center[1].clear();
    let frames = super::super::world_sprite_frames(&art, &[], &[], &[u], camera(), 0);
    assert_eq!(frames.len(), 1);
}

#[wasm_bindgen_test]
fn native_town_center_front_overhang_clears_its_flat_footprint() {
    let mut art = art();
    art.town_center[0][0].size = [195.0, 200.0];
    art.town_center[0][0].anchor = [103.0, 136.0];
    let unit = unit(SceneUnitKind::TownCenter);
    let frames = super::super::world_sprite_frames(&art, &[], &[], &[unit], camera(), 0);
    let (_, frame, depth, _) = frames
        .iter()
        .find(|(_, frame, _, _)| frame.atlas.uv[0] == 500.0 / 4096.0)
        .unwrap();
    let foreground = unit.position[0]
        + unit.position[1]
        + f64::from(frame.size[1] - frame.anchor[1]) / (aoe_core::ISO_TILE_HEIGHT / 2.0);
    assert!(
        *depth > foreground,
        "native front columns and foundation must clear flat ground"
    );
}

#[wasm_bindgen_test]
fn town_center_roofs_use_individual_attachments_and_sit_behind_the_main_structure() {
    let art = art();
    let frames = super::super::world_sprite_frames(
        &art,
        &[],
        &[],
        &[unit(SceneUnitKind::TownCenter)],
        camera(),
        0,
    );
    let index = |source| {
        frames
            .iter()
            .position(|f| f.1.atlas.uv[0] == source as f32 / 4096.0)
            .unwrap()
    };
    let main = index(500);
    for (source, offset) in [(501, 48.0), (504, 24.0), (502, 24.0), (506, -24.0)] {
        let roof = index(source);
        assert!(
            !matches!(source, 501 | 504) || roof < main,
            "main balcony must occlude its roof attachments"
        );
        assert!(
            (frames[main].1.anchor[1] - frames[roof].1.anchor[1] - offset * 4.0 / 3.0).abs()
                < 0.001
        );
    }
}

#[wasm_bindgen_test]
fn building_ground_is_darkened_on_the_shared_surface_without_extra_layers() {
    let art = art();
    let terrain = (-6..6)
        .flat_map(|y| {
            (-6..6).map(move |x| SceneTerrain {
                position: [f64::from(x) + 0.5, f64::from(y) + 0.5],
                material: 0,
                elevation_meters: 0.0,
                surface: SceneTerrainSurface::flat(0.0),
                appearance: None,
            })
        })
        .collect::<Vec<_>>();
    let camera = camera();
    let surfaces = crate::projected_surface_triangles(&terrain, camera);
    let layers = crate::game_renderer::world_layers::direct_world_layers(
        surfaces.clone(),
        &art,
        &terrain,
        &[],
        &[unit(SceneUnitKind::TownCenter)],
        camera,
        0,
    );
    assert_eq!(
        layers
            .iter()
            .filter(|layer| matches!(layer, WorldLayer::Surface(_)))
            .count(),
        surfaces.len()
    );
    let changed = layers
        .iter()
        .filter_map(|layer| match layer {
            WorldLayer::Surface(face) => Some(face),
            _ => None,
        })
        .zip(&surfaces)
        .filter(|(after, before)| {
            assert_eq!(
                after.points.map(|p| p.world),
                before.points.map(|p| p.world)
            );
            assert!(
                after
                    .color
                    .iter()
                    .zip(before.color)
                    .all(|(after, before)| *after <= before)
            );
            let center =
                [0, 1].map(|axis| before.points.iter().map(|p| p.world[axis]).sum::<f64>() / 3.0);
            if center
                .into_iter()
                .any(|coordinate| (coordinate - 0.5).abs() >= 3.4)
            {
                assert_eq!(
                    after.color, before.color,
                    "ground outside the footprint halo is untouched"
                );
            }
            after.color != before.color
        })
        .count();
    assert!(
        changed > 0,
        "a building must wear the terrain below and around its footprint"
    );
}

#[wasm_bindgen_test]
fn construction_stages_and_finished_building_select_native_frames() {
    assert_eq!(
        std::mem::size_of::<SceneUnit>(),
        32,
        "building roles preserve the frame unit layout budget"
    );
    let mut art = art();
    let mut stages = Vec::new();
    for i in 0..3 {
        let mut frame = art.village[0][0];
        frame.atlas.uv[0] = 0.1 + i as f32 * 0.1;
        stages.push(frame);
    }
    art.buildings[10] = stages;
    art.buildings[1] = vec![art.village[1][0]];
    for (progress, slot, expected) in [
        (0_u16, 10, 0.1),
        (500, 10, 0.2),
        (999, 10, 0.3),
        (1000, 1, art.village[1][0].atlas.uv[0]),
    ] {
        let mut u = unit(SceneUnitKind::building(slot));
        u.facing = (u32::from(progress) * 3 / 1000).min(2) as u8;
        let mut emitted = Vec::new();
        building(&art, u, camera(), 0, &mut |entry| emitted.push(entry));
        assert_eq!(emitted.len(), 1);
        assert!((emitted[0].1.atlas.uv[0] - expected).abs() < 0.0001);
        assert_eq!(u.kind.building_side(), Some(2.0));
    }
    let mut emitted = Vec::new();
    building(
        &art,
        unit(SceneUnitKind::building(5)),
        camera(),
        0,
        &mut |entry| emitted.push(entry),
    );
    assert!(emitted.is_empty(), "a farm is ground, not a billboard");
}

#[wasm_bindgen_test]
fn farm_has_textured_ground_even_before_map_chunks_arrive() {
    let mut art = art();
    art.buildings[5] = vec![art.village[0][0]];
    let farm = unit(SceneUnitKind::building(5));
    let layers = crate::game_renderer::world_layers::direct_world_layers(
        std::iter::empty(),
        &art,
        &[],
        &[],
        &[farm],
        camera(),
        0,
    );
    assert!(
        layers
            .iter()
            .any(|l| matches!(l,WorldLayer::Surface(t) if t.texture_uv.is_some()))
    );
    assert!(
        !layers
            .iter()
            .any(|l| matches!(l, WorldLayer::Sprite(_, _, _, _))),
        "farm texture belongs to ground triangles"
    );
}

#[wasm_bindgen_test]
fn layered_mill_composes_static_body_and_every_sail_frame_and_culls_animation() {
    let mut art = art();
    art.buildings[4] = (0..44)
        .map(|i| {
            let mut f = art.village[0][0];
            f.atlas.uv[0] = i as f32 / 4096.0;
            f
        })
        .collect();
    let mill = unit(SceneUnitKind::Mill);
    for tick in 0..86 {
        let mut emitted = Vec::new();
        building(&art, mill, camera(), tick, &mut |entry| emitted.push(entry));
        assert_eq!(emitted.len(), 2);
        assert_eq!(emitted[0].1.atlas.uv[0], 0.0);
        assert_eq!(emitted[1].1.atlas.uv[0], (1 + tick % 43) as f32 / 4096.0);
    }
    assert!(crate::scene_unit_animates(&art, mill, camera()));
    let mut offscreen = mill;
    offscreen.position = [1000.0; 2];
    assert!(!crate::scene_unit_animates(&art, offscreen, camera()));
}

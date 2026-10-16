use super::*;
use aoe_core::EntityId;
use wasm_bindgen_test::wasm_bindgen_test;

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
        village: std::array::from_fn(|group| {
            (0..75).map(|index| frame(group * 100 + index)).collect()
        }),
        town_center: std::array::from_fn(|group| vec![frame(500 + group)]),
        walking: vec![frame(900); 50],
        standing: vec![frame(901); 50],
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
        (SceneUnitKind::VillagerWorking, 207),
        (SceneUnitKind::VillagerCarrying, 300),
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
    for source in [501.0, 504.0] {
        let roof = frames
            .iter()
            .find(|f| f.1.atlas.uv[0] == source / 4096.0)
            .unwrap();
        assert!((main.1.anchor[1] - roof.1.anchor[1] - 64.0).abs() < 0.001);
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

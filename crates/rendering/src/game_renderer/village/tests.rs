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
    art.town_center[1].clear();
    let frames = super::super::world_sprite_frames(&art, &[], &[], &[u], camera(), 0);
    assert_eq!(frames.len(), 1);
}

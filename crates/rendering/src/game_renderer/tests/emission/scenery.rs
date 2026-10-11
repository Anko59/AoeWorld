//! Scenery sprites: cliff pieces drawn as ground-lying, depth-sorted bands.
use super::super::*;
use wasm_bindgen_test::wasm_bindgen_test;

fn art() -> GameArt {
    // Twenty-five slots like the pack: a 291x147 face with its hotspot in the
    // middle of a three-by-three tile footprint.
    let cliff_faces = (0..25)
        .map(|slot| GameFrame {
            atlas: crate::AtlasAddress {
                page: 2,
                uv: [0.25, 0.5, 0.125, 0.0625 + slot as f32 * 1e-4],
            },
            size: [291.0, 147.0],
            anchor: [145.0, 73.0],
        })
        .collect();
    GameArt {
        walking: Vec::new(),
        standing: Vec::new(),
        grass: Vec::new(),
        terrain: Default::default(),
        terrain_topology: [None; 7],
        water_depth: Default::default(),
        resources: Default::default(),
        tree_shadows: Vec::new(),
        tree_families: Default::default(),
        scenery: crate::SceneryArt { cliff_faces },
    }
}

fn band(facing: u8, band: u8, lift: u8) -> SceneResource {
    let offset = SceneResource::cliff_band_offset(band);
    SceneResource {
        id: 9,
        position: [10.5 + offset, 10.5 + offset],
        kind: SceneResource::CLIFF,
        visual_variant: 0,
        visual_family: SceneResource::cliff_family(facing, band, lift),
        elevation_meters: 0.0,
    }
}

fn camera() -> SceneCamera {
    SceneCamera {
        center: [10.5, 10.5],
        zoom: 1.0,
        viewport: [1024.0, 768.0],
        focus_elevation_meters: 0.0,
    }
}

#[wasm_bindgen_test]
fn a_cliff_piece_is_eight_shadowless_bands_that_cover_its_footprint_once() {
    let art = art();
    let resources = (0..SceneResource::CLIFF_BANDS)
        .map(|index| band(0, index, 0))
        .collect::<Vec<_>>();
    let sprites = world_sprite_frames(&art, &[], &resources, &[], camera(), 0);
    // One body per band and no shadow sprite.
    assert_eq!(sprites.len(), usize::from(SceneResource::CLIFF_BANDS));
    let scale = (aoe_core::ISO_TILE_WIDTH / 97.0) as f32;
    let (mut top, mut bottom, mut previous_depth) = (f32::MAX, f32::MIN, f64::MIN);
    let mut previous_bottom = None;
    for (sprite, frame, depth, id) in &sprites {
        assert_eq!(*id, 9);
        assert!(sprite.color == [1.0; 4] && sprite.pages[0] == 2);
        assert!((frame.size[0] - 291.0 * scale).abs() < 1e-3);
        let center_y = (1.0 - sprite.position[1]) * 384.0;
        let (first, last) = (
            center_y - frame.size[1] / 2.0,
            center_y + frame.size[1] / 2.0,
        );
        // Bands run back to front, each reaching under the one behind it.
        if let Some(previous) = previous_bottom {
            assert!(first < previous && last > previous);
        }
        assert!(*depth > previous_depth);
        (previous_bottom, previous_depth) = (Some(last), *depth);
        (top, bottom) = (top.min(first), bottom.max(last));
        // The piece is centred on its anchor tile horizontally.
        assert!(((sprite.position[0] + 1.0) * 512.0 - 512.0 - 0.5 * scale).abs() < 1.0);
    }
    // Together the bands show the whole 147-row face, enlarged to the ground.
    assert!((bottom - top - 147.0 * scale).abs() < 1.5, "{top} {bottom}");
    assert!((384.0 - top - 73.0 * scale).abs() < 1.5);
    let rows = sprites
        .iter()
        .map(|(sprite, _, _, _)| sprite.uv[3])
        .sum::<f32>();
    assert!(
        rows >= 0.0625 && rows < 0.0625 * 1.25,
        "overlap only: {rows}"
    );
}

#[wasm_bindgen_test]
fn cliff_bands_mirror_lift_and_vanish_without_art_or_away_from_the_camera() {
    let art = art();
    let frame = |facing, lift| {
        world_sprite_frames(&art, &[], &[band(facing, 4, lift)], &[], camera(), 0)
            .into_iter()
            .next()
    };
    let (southwest, _, depth, _) = frame(1, 0).expect("southwest face");
    let (southeast, _, lifted, _) = frame(7, 2).expect("southeast face");
    assert!(southwest.uv[2] > 0.0 && southeast.uv[2] < 0.0);
    assert_eq!(southeast.uv[0], southwest.uv[0] + southwest.uv[2]);
    assert!((lifted - depth - 2.0 * SceneResource::CLIFF_DEPTH_LIFT).abs() < 1e-9);
    // The band only just covers the ground it lies on.
    let ground = crate::surface_mesh::surface_depth([
        band(1, 4, 0).position[0],
        band(1, 4, 0).position[1],
        0.0,
    ]);
    assert!((depth - ground - SceneResource::CLIFF_DEPTH_BIAS).abs() < 1e-9);
    for away in 3..=5 {
        assert!(frame(away, 0).is_none());
    }
    let mut partial = self::art();
    partial.scenery.cliff_faces.truncate(11);
    assert!(scene_resource_frame(&partial, band(0, 4, 0)).is_none());
    assert!(scene_resource_frame(&art, band(0, 4, 0)).is_some());
}

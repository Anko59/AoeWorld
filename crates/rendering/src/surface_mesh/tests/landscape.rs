use super::appearance_tests::camera;
use super::*;

#[wasm_bindgen_test]
fn landscape_metadata_has_neutral_legacy_packets_and_bounded_order_stable_lod() {
    let appearance = crate::SceneTerrainAppearance {
        floor_strength: 650,
        canopy_strength: 650,
        palette: 1,
        exposure: 2,
        height_band: 3,
    };
    assert_eq!(landscape::pack(None), 0);
    assert_eq!(
        landscape::pack(Some(crate::SceneTerrainAppearance {
            floor_strength: 0,
            canopy_strength: 0,
            palette: 0,
            exposure: 0,
            height_band: 0,
        })),
        1
    );
    let word = landscape::pack(Some(appearance));
    assert_eq!(word & 1, 1);
    assert_eq!((word >> 4) & 1023, 650);
    assert_eq!((word >> 14) & 1023, 650);
    assert_eq!((word >> 24) & 3, 2);
    assert_eq!((word >> 26) & 7, 3);
    assert_eq!(word >> 29, 0);
    let mut terrain = (0..128)
        .flat_map(|y| {
            (0..128).map(move |x| SceneTerrain {
                position: [f64::from(x) + 0.5, f64::from(y) + 0.5],
                material: 0,
                elevation_meters: 0.0,
                surface: SceneTerrainSurface::flat(0.0),
                appearance: Some(crate::SceneTerrainAppearance {
                    floor_strength: if x % 7 == 0 { 1000 } else { 0 },
                    canopy_strength: if x % 7 == 0 { 1000 } else { 0 },
                    ..appearance
                }),
            })
        })
        .collect::<Vec<_>>();
    let view = camera([64.0; 2], 0.1, [4096.0, 2160.0]);
    let first = projected_surface_triangles(&terrain, view);
    assert!(first.len() <= MAX_SURFACE_TRIANGLES);
    terrain.reverse();
    let reverse = projected_surface_triangles(&terrain, view);
    assert_eq!(
        first
            .iter()
            .map(|t| (t.tile, t.appearance))
            .collect::<Vec<_>>(),
        reverse
            .iter()
            .map(|t| (t.tile, t.appearance))
            .collect::<Vec<_>>()
    );
    for sample in &mut terrain {
        sample.appearance = None;
    }
    let legacy = projected_surface_triangles(&terrain, view);
    assert_eq!(first.len(), legacy.len());
    for (new, old) in first.iter().zip(legacy) {
        assert_eq!(new.points.map(|p| p.world), old.points.map(|p| p.world));
        assert_eq!(new.pickable, old.pickable);
        assert_eq!(old.appearance, 0);
    }
}

#[wasm_bindgen_test]
fn landscape_floor_selects_grass_and_forest_fallback_without_procedural_recoloring() {
    let frame = |page| GameFrame {
        atlas: crate::AtlasAddress {
            page,
            uv: [0.0, 0.0, 0.01, 0.01],
        },
        size: [97.0, 49.0],
        anchor: [48.0, 24.0],
    };
    let mut art = test_art(frame(0));
    art.terrain[6].clear();
    art.terrain[2] = vec![frame(1)];
    let sample = SceneTerrain {
        position: [0.5; 2],
        material: 6,
        elevation_meters: 0.0,
        surface: SceneTerrainSurface::flat(0.0),
        appearance: Some(crate::SceneTerrainAppearance {
            floor_strength: 650,
            canopy_strength: 650,
            palette: 0,
            exposure: 0,
            height_band: 0,
        }),
    };
    let mut triangles =
        projected_surface_triangles(&[sample], camera([0.5; 2], 1.0, [256.0, 128.0]));
    apply_terrain_textures(&mut triangles, &art);
    assert!(
        triangles
            .iter()
            .all(|t| t.texture_uv == Some(frame(0).atlas)
                && t.texture_blend == Some([frame(1).atlas, frame(0).atlas]))
    );
    for material in [4, 5, 7, 8, 9, 10] {
        let mut raw = projected_surface_triangles(
            &[SceneTerrain { material, ..sample }],
            camera([0.5; 2], 1.0, [256.0, 128.0]),
        );
        apply_terrain_textures(&mut raw, &art);
        assert!(raw.iter().all(|t| t.texture_blend.is_none()));
        assert!(
            raw.iter()
                .all(|t| t.appearance == landscape::pack(sample.appearance))
        );
        assert!(
            raw.iter()
                .all(|t| crate::web::surface_instance(t, [256.0, 128.0], 0.0).pages[3] == 0)
        );
    }
}

use super::*;
use crate::surface_mesh::directory::fixture;
use crate::{AtlasAddress, BuildingArt, PreparedTerrain, TerrainTopology};

fn camera() -> crate::SceneCamera {
    fixture::camera(0)
}
fn terrain() -> Vec<crate::SceneTerrain> {
    (-8..8)
        .flat_map(|y| {
            (-8..8).map(move |x| crate::SceneTerrain {
                position: [f64::from(x) + 0.5, f64::from(y) + 0.5],
                material: if x < 0 { 0 } else { 2 },
                elevation_meters: 0.0,
                surface: crate::SceneTerrainSurface::flat(0.0),
                appearance: None,
            })
        })
        .collect()
}

#[wasm_bindgen_test]
fn prepared_terrain_keeps_materials_and_demand_across_actor_streaming() {
    let mut art = fixture::art();
    let topology = TerrainTopology::Dds {
        size: [1024; 2],
        period: [10; 2],
        level: 0,
    };
    let mut frame = art.terrain[0][0];
    frame.stream = 1;
    art.terrain[0] = vec![frame; topology.dds_frames().unwrap()];
    art.terrain_topology[0] = Some(topology);
    art.resident = vec![None; 128];
    let terrain = terrain();
    let source = projected_surface_triangles(&terrain, camera());
    let mut expected_ids = crate::scene_terrain_frames(&mut art, &terrain, camera());
    expected_ids.extend(crate::scene_cliff_native_frames(&art, &source));
    let mut expected = source.clone();
    apply_terrain_textures(&mut expected, &art);
    let prepared = PreparedTerrain::new(&mut art, &terrain, &source, camera());
    assert_eq!(prepared.required(), expected_ids);
    assert!(!prepared.required().is_empty());
    assert!(prepared.resident_bytes() >= source.len() * size_of::<ProjectedSurfaceTriangle>());
    // Unrelated native animation traffic must not alter the retained ground.
    for page in [2, 4, 8] {
        art.resident[127] = Some(AtlasAddress {
            page,
            ..AtlasAddress::WHITE
        });
        assert_eq!(
            format!("{:?}", prepared.surfaces(&art).collect::<Vec<_>>()),
            format!("{expected:?}")
        );
        assert_eq!(prepared.required(), expected_ids);
    }
    // A different view gets a new density choice and its own demand.
    let far_camera = crate::SceneCamera {
        center: [10000.0; 2],
        zoom: 0.01,
        ..camera()
    };
    let far_source = projected_surface_triangles(&terrain, far_camera);
    let far = PreparedTerrain::new(&mut art, &terrain, &far_source, far_camera);
    assert!(far.required().is_empty());
}

#[wasm_bindgen_test]
fn prepared_terrain_resolves_streamed_cliffs_after_upload_eviction_and_relocation() {
    let mut art = fixture::art();
    let mut cliff = art.terrain[0][0];
    cliff.stream = 1;
    art.scenery.cliff_faces = vec![cliff; 25];
    art.environment.push(BuildingArt {
        unit_id: 264,
        footprint: [1.0; 2],
        foundation_material: None,
        parts: Vec::new(),
        construction: Vec::new(),
        states: Vec::new(),
        damage: Vec::new(),
    });
    art.resident = vec![None];
    let terrain = terrain();
    let mut source = projected_surface_triangles(&terrain, camera());
    source[0].skirt = true;
    source[0].tint = 13;
    let prepared = PreparedTerrain::new(&mut art, &terrain, &source, camera());
    assert!(prepared.required().contains(&1));
    for address in [
        None,
        Some(AtlasAddress::WHITE),
        None,
        Some(AtlasAddress {
            page: 8,
            ..AtlasAddress::WHITE
        }),
    ] {
        art.resident[0] = address;
        let mut expected = source.clone();
        apply_terrain_textures(&mut expected, &art);
        assert_eq!(
            format!("{:?}", prepared.surfaces(&art).collect::<Vec<_>>()),
            format!("{expected:?}")
        );
    }
}

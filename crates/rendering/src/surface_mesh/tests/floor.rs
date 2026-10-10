use super::*;
use std::collections::BTreeMap;

fn metadata(floor_strength: u16) -> crate::SceneTerrainAppearance {
    crate::SceneTerrainAppearance {
        floor_strength,
        canopy_strength: 0,
        palette: 5,
        exposure: 0,
        height_band: 0,
    }
}
fn value(x: i32, y: i32) -> u16 {
    (x.wrapping_mul(73).wrapping_add(y * 151).rem_euclid(1001)) as u16
}

#[wasm_bindgen_test]
fn floor_vertices_share_incident_support_across_negative_chunk_edges_and_lods() {
    let mut terrain = (-64..64)
        .flat_map(|y| {
            (-64..64).map(move |x| SceneTerrain {
                position: [f64::from(x) + 0.5, f64::from(y) + 0.5],
                material: 2,
                elevation_meters: 0.0,
                surface: SceneTerrainSurface::flat(0.0),
                appearance: Some(metadata(value(x, y))),
            })
        })
        .collect::<Vec<_>>();
    let mut previous = BTreeMap::new();
    for (zoom, viewport) in [(1.0, [640.0, 480.0]), (0.1, [4096.0, 2160.0])] {
        let camera = super::appearance_tests::camera([0.0; 2], zoom, viewport);
        for triangulation in 0..2 {
            for t in &mut terrain {
                t.surface.triangulation = triangulation;
            }
            let triangles = projected_surface_triangles(&terrain, camera);
            assert!(triangles.len() <= MAX_SURFACE_TRIANGLES);
            let mut seen = BTreeMap::new();
            for t in &triangles {
                let floors = t.floor_strengths.expect("landscape displayed vertex field");
                for (p, floor) in t.points.iter().zip(floors) {
                    let key = [p.world[0] as i32, p.world[1] as i32];
                    if let Some(old) = seen.insert(key, floor) {
                        assert_eq!(old, floor);
                    }
                    let support = [(-1, -1), (-1, 0), (0, -1), (0, 0)]
                        .into_iter()
                        .filter_map(|(dx, dy)| {
                            let (x, y) = (key[0] + dx, key[1] + dy);
                            ((-64..64).contains(&x) && (-64..64).contains(&y))
                                .then(|| u32::from(value(x, y)))
                        })
                        .collect::<Vec<_>>();
                    let expected = (support.iter().sum::<u32>() + support.len() as u32 / 2)
                        / support.len().max(1) as u32;
                    assert_eq!(
                        floor,
                        landscape::quantized_floor(expected as u16),
                        "world vertex {key:?}"
                    );
                    if let Some(old) = previous.get(&key) {
                        assert_eq!(*old, floor);
                    }
                }
            }
            previous.extend(seen);
            terrain.reverse();
            let reversed = projected_surface_triangles(&terrain, camera);
            assert_eq!(triangles.len(), reversed.len());
            for (a, b) in triangles.iter().zip(reversed) {
                assert_eq!(a.points.map(|p| p.world), b.points.map(|p| p.world));
                assert_eq!(a.floor_strengths, b.floor_strengths);
                assert_eq!(a.appearance, b.appearance);
            }
        }
    }
}

#[wasm_bindgen_test]
fn floor_numeric_packet_is_exact_bounded_and_survives_depth_normalization() {
    assert_eq!(std::mem::size_of::<crate::web::Sprite>(), 112);
    assert_eq!(std::mem::size_of::<Option<[u8; 3]>>(), 4);
    assert_eq!(std::mem::size_of::<ProjectedSurfaceTriangle>(), 240);
    for v in 0..=u16::MAX {
        assert_eq!(
            landscape::quantized_floor(v),
            ((u32::from(v.min(1000)) * 255 + 500) / 1000) as u8
        );
    }
    for v in 0..=u8::MAX {
        let packed = landscape::pack_floors([v, 255 - v, 255]);
        assert!(packed <= 16_777_215.0);
        let raw = packed as u32;
        assert_eq!(raw as f32, packed);
        assert_eq!((raw & 255) as u8, v);
        assert_eq!(((raw >> 8) & 255) as u8, 255 - v);
        assert_eq!(raw >> 16, 255);
    }
    assert_eq!(landscape::pack_floors([255; 3]), 16_777_215.0);
    let terrain = [SceneTerrain {
        position: [0.5; 2],
        material: 2,
        elevation_meters: 0.0,
        surface: SceneTerrainSurface::flat(0.0),
        appearance: Some(metadata(650)),
    }];
    let mut triangles = projected_surface_triangles(
        &terrain,
        super::appearance_tests::camera([0.5; 2], 1.0, [128.0; 2]),
    );
    let frame = GameFrame {
        atlas: crate::AtlasAddress {
            page: 0,
            uv: [0.0, 0.0, 0.01, 0.01],
        },
        size: [1.0; 2],
        anchor: [0.0; 2],
    };
    apply_terrain_textures(&mut triangles, &test_art(frame));
    let face = &mut triangles[0];
    face.floor_strengths = Some([0, 128, 255]);
    let mut packet = crate::web::surface_instance(face, [128.0; 2], 0.0);
    assert_eq!(
        packet.pages[3],
        face.appearance | landscape::INTERPOLATED_FLOOR
    );
    let packed = packet.depths[3];
    crate::web::normalize_depths(std::slice::from_mut(&mut packet));
    assert_eq!(packet.depths[3], packed);
    face.floor_strengths = None;
    let constant = crate::web::surface_instance(face, [128.0; 2], 0.0);
    assert_eq!(constant.pages[3], face.appearance);
    assert_eq!(constant.depths[3], 0.0);
    face.appearance = 0;
    face.floor_strengths = Some([255; 3]);
    let bare = crate::web::surface_instance(face, [128.0; 2], 0.0);
    assert_eq!(bare.pages[3], 0);
    assert_eq!(bare.depths[3], 0.0);
}

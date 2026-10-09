//! Pure qualifier/address tests supplement, but do not replace, real backend pixels.
use super::fixtures as fixture;
use super::*;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn world_provenance_keeps_old_abis_and_exact_signed_native_layout() {
    assert_eq!(std::mem::size_of::<ProjectedSurfaceTriangle>(), 240);
    assert_eq!(std::mem::size_of::<crate::Sprite>(), 112);
    let mut data = fixture::Fixture::new();
    let table = aoe_assets::catalog::packing::terrain_lookup::validated_table(&data.atlas)
        .unwrap()
        .unwrap();
    assert_eq!(table.metadata().total, 16);
    for origin in [[-7, 11], [i32::MIN, i32::MAX], [i32::MAX, i32::MIN], [0, 0]] {
        for group in [0, 6] {
            let selected = table
                .metadata()
                .frame_index(group, origin[0], origin[1])
                .unwrap();
            let base = if group == 0 { 0 } else { 6 };
            assert_eq!(
                usize::from(selected),
                base + fixture::selector(group, origin)
            );
            let frame = table.descriptor(selected).unwrap();
            assert_eq!((frame.width, frame.height), (97, 49));
            assert!(frame.page <= 1);
        }
    }
    drop(table);
    for case in fixture::cases() {
        let faces = fixture::faces(&data.art, case, false);
        assert!(faces.iter().all(|face| face.world_texture().is_some()));
        assert!(
            fixture::legacy(&faces)
                .iter()
                .all(|face| face.world_texture().is_none())
        );
        let original = faces[0];
        for invalid in 0..8 {
            let mut face = original;
            match invalid {
                0 => face.skirt = true,
                1 => face.tint = 4,
                2 => face.material = 7,
                3 => face.appearance = 0,
                4 => face.texture_mode = 4,
                5 => face.points[1].world[0] += 0.25,
                6 => face.points[0].world[1] = f64::NAN,
                _ => face.texture_tile[0] = face.texture_tile[0].wrapping_add(1),
            }
            assert!(
                qualify(&face, &data.art, [0, 6]).is_none(),
                "accepted invalid case {invalid}"
            );
        }
        let mut absent = original;
        data.art.terrain_world = None;
        apply_terrain_textures(std::slice::from_mut(&mut absent), &data.art);
        assert!(absent.world_texture().is_none());
        data.art.terrain_world = original.world_texture().map(|world| world.checksum);
    }
    // Missing forest art makes primary dirt and bed dirt identical. Admission
    // downgrade must reproduce the ORIGINAL one-layer packet bit for bit.
    let mut equal = fixture::faces(&data.art, fixture::cases()[0], false)[0];
    data.art.terrain[2] = data.art.terrain[0].clone();
    data.art.terrain_topology[2] = data.art.terrain_topology[0];
    data.art.terrain[6].clear();
    data.art.terrain_topology[6] = None;
    equal.material = 2;
    apply_terrain_textures(std::slice::from_mut(&mut equal), &data.art);
    assert_eq!(equal.world_texture().unwrap().groups, [2, 2]);
    let mut old = equal;
    let key = data.art.terrain_world;
    data.art.terrain_world = None;
    apply_terrain_textures(std::slice::from_mut(&mut old), &data.art);
    data.art.terrain_world = key;
    assert!(old.texture_blend.is_none());
    let expected = crate::web::surface_instance(&old, [128.0; 2], 0.0);
    let mut packet = crate::web::surface_instance(&equal, [128.0; 2], 0.0);
    crate::web::retain_world_packet(&mut packet, None);
    assert_eq!(bytemuck::bytes_of(&packet), bytemuck::bytes_of(&expected));
    equal.retain_world_texture(None);
    let triangle_packet = crate::web::surface_instance(&equal, [128.0; 2], 0.0);
    assert_eq!(
        bytemuck::bytes_of(&triangle_packet),
        bytemuck::bytes_of(&expected)
    );
    let old_key = data.art.terrain_world.unwrap();
    data.layout(true);
    assert_ne!(data.art.terrain_world.unwrap(), old_key);
    let mut case = fixture::cases()[0];
    case.size = 4;
    case.zoom = true;
    let points = fixture::probes(case);
    case.zoom = false;
    for (lo, hi) in points.into_iter().zip(fixture::probes(case)) {
        let mut low = case;
        low.zoom = true;
        assert_eq!(fixture::expected(low, lo), fixture::expected(case, hi));
    }
}

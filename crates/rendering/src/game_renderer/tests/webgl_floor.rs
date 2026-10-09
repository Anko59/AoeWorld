use super::*;

#[wasm_bindgen_test]
fn webgl_restored_native_forest_soil_endpoints_gradient_seam_and_dirt_primary() {
    use crate::surface_mesh::floor as fixture;
    let (canvas, mut renderer) = target(&fixture::forest_atlas());
    for material in [0, 2, 6] {
        for (floor, gradient) in [(0, false), (650, false), (1000, false), (650, true)] {
            let mut faces = fixture::forest_faces(material, floor, gradient);
            for _ in 0..2 {
                let mut packets = faces.map(|face| instance(&face));
                render(&mut renderer, &mut packets);
                for [x, y] in fixture::FOREST_PROBES {
                    assert_pixel(
                        &canvas,
                        x,
                        y,
                        fixture::forest_expected(material, floor, gradient, x),
                    );
                }
                faces.reverse();
            }
        }
    }
}

#[wasm_bindgen_test]
fn webgl_shared_floor_gradient_seam_and_old_packet_fallback() {
    use crate::surface_mesh::floor as fixture;
    let (canvas, mut renderer) = target(&fixture::atlas());
    for interpolated in [true, false] {
        let mut faces = fixture::faces(interpolated);
        for _ in 0..2 {
            let mut packets = faces.map(|face| instance(&face));
            render(&mut renderer, &mut packets);
            for [x, y] in fixture::PROBES {
                assert_pixel(&canvas, x, y, fixture::expected(x, interpolated));
            }
            faces.reverse();
        }
    }
}

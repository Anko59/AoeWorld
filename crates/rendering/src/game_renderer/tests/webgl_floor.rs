use super::*;

#[wasm_bindgen_test]
fn webgl_shared_floor_gradient_seam_and_manual_packet_fallback() {
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

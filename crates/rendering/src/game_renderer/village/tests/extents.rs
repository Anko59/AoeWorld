use super::*;

#[test]
fn extents_include_signed_hotspots_offsets_and_native_world_scale() {
    let mut extent = [0.0; 2];
    include(
        &mut extent,
        [10.0, 20.0],
        [-7.0, 30.0],
        [100.0, -50.0],
        4.0 / 3.0,
    );
    assert!((extent[0] - 164.0).abs() < 1e-6);
    assert!((extent[1] - (80.0 * 4.0 / 3.0 + 8.0)).abs() < 1e-6);
}

#[test]
fn extents_cover_mirrored_bodies_and_offset_alpha_shadows() {
    for width in [1.0, 40.0, 2048.0] {
        for anchor in [-300.0, 0.0, 2500.0] {
            let size = [width, 600.0];
            let anchor = [anchor, -90.0];
            let offset = [-250.0_f32, 400.0];
            let scale = 4.0 / 3.0;
            let mut extent = [0.0; 2];
            include(&mut extent, size, anchor, offset, scale);
            for axis in 0..2 {
                for mirrored in [false, true] {
                    let a = if mirrored {
                        size[axis] - anchor[axis]
                    } else {
                        anchor[axis]
                    };
                    for endpoint in [0.0, size[axis]] {
                        let body = f64::from(endpoint - a + offset[axis]) * scale;
                        let shadow = body * if axis == 0 { 0.88 } else { 0.2 }
                            + if axis == 0 { 6.0 } else { 4.0 };
                        assert!(body.abs() <= extent[axis]);
                        assert!(shadow.abs() <= extent[axis]);
                    }
                }
            }
        }
    }
}

#[test]
fn malformed_metadata_disables_rejection_instead_of_hiding_pixels() {
    let mut extent = [512.0; 2];
    include(&mut extent, [f32::NAN, 2.0], [0.0; 2], [0.0; 2], 1.0);
    assert!(extent[0].is_infinite());
    assert_eq!(extent[1], 512.0);
}

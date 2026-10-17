use super::*;
use crate::surface_mesh::directory::fixture;

#[wasm_bindgen_test]
fn canvas_shallow_water_fades_into_sand_at_two_zooms() {
    use crate::surface_mesh::mix::fixture as shore;
    let (canvas, context) = target_canvas().expect("browser Canvas target");
    let mut presentation = CanvasPresentation::new(canvas.width(), canvas.height());
    let atlas = shore::atlas();
    for zoom in [0.2, 0.4] {
        let layers = shore::shore_scene(zoom)
            .into_iter()
            .map(WorldLayer::Surface)
            .collect::<Vec<_>>();
        let mut camera = shore::camera(0);
        camera.zoom = zoom;
        canvas_depth::render_canvas_world(
            &canvas,
            &context,
            &atlas,
            &mut presentation,
            &layers,
            camera,
            false,
        )
        .unwrap();
        shore::assert_shore(
            shore::SHORE_PROBES.map(|[x, y]| pixel(&presentation.color_buffer, x, y)),
        );
        shore::assert_soft(0, &|x, y| pixel(&presentation.color_buffer, x, y));
    }
}

/// The same world point shows the same native texel at two zooms and at two
/// mesh LODs; coarse patches tile by world coordinate, not by patch origin.
#[wasm_bindgen_test]
fn canvas_terrain_texels_are_world_anchored_across_zoom_and_mesh_lod() {
    let (canvas, context) = target_canvas().expect("browser Canvas target");
    let mut presentation = CanvasPresentation::new(canvas.width(), canvas.height());
    let atlas = fixture::atlas();
    let mut dirt = Vec::new();
    for case in 0..fixture::CASES {
        let layers = fixture::scene(case)
            .into_iter()
            .map(WorldLayer::Surface)
            .collect::<Vec<_>>();
        canvas_depth::render_canvas_world(
            &canvas,
            &context,
            &atlas,
            &mut presentation,
            &layers,
            fixture::camera(case),
            false,
        )
        .unwrap();
        for (probe, [x, y]) in fixture::PROBES.into_iter().enumerate() {
            let actual = pixel(&presentation.color_buffer, x, y);
            fixture::assert_probe(case, probe, actual, 0);
            if matches!(case, 2 | 3) {
                dirt.push(actual);
            }
        }
    }
    // Fine and coarse meshes under one camera: no texel changes at an LOD switch.
    let (fine, coarse) = dirt.split_at(dirt.len() / 2);
    assert_eq!(fine, coarse);
}

/// Every pair of ground materials meets in a soft, noisy band: the reference
/// blend at each probe, pure sheets away from the border, no hard step.
#[wasm_bindgen_test]
fn canvas_blends_every_ground_material_pair_across_a_noisy_border() {
    use crate::surface_mesh::mix::fixture as mix;
    let (canvas, context) = target_canvas().expect("browser Canvas target");
    let mut presentation = CanvasPresentation::new(canvas.width(), canvas.height());
    let atlas = mix::atlas();
    for case in 0..mix::CASES {
        let layers = mix::scene(case)
            .into_iter()
            .map(WorldLayer::Surface)
            .collect::<Vec<_>>();
        canvas_depth::render_canvas_world(
            &canvas,
            &context,
            &atlas,
            &mut presentation,
            &layers,
            mix::camera(case),
            false,
        )
        .unwrap();
        let row = mix::PROBES.map(|[x, y]| pixel(&presentation.color_buffer, x, y));
        for (probe, actual) in row.into_iter().enumerate() {
            mix::assert_probe(case, probe, actual, 1);
        }
        mix::assert_soft(case, &|x, y| pixel(&presentation.color_buffer, x, y));
    }
}

#[wasm_bindgen_test]
fn canvas_four_material_junction_has_matching_pixels_on_every_shared_edge() {
    use crate::surface_mesh::mix::fixture::{self as mix, junction};
    let (canvas, context) = target_canvas().expect("browser Canvas target");
    let mut presentation = CanvasPresentation::new(canvas.width(), canvas.height());
    let atlas = mix::atlas();
    for pair in junction::edge_pairs() {
        let pixels = pair.map(|face| {
            canvas_depth::render_canvas_world(
                &canvas,
                &context,
                &atlas,
                &mut presentation,
                &[WorldLayer::Surface(face)],
                junction::camera(),
                false,
            )
            .unwrap();
            pixel(&presentation.color_buffer, 64, 64)
        });
        junction::assert_edge(&pair, pixels);
    }
}

#[wasm_bindgen_test]
fn canvas_snow_is_white_ice_stays_blue_and_their_ground_borders_are_soft() {
    use crate::surface_mesh::mix::fixture as mix;
    let (canvas, context) = target_canvas().expect("browser Canvas target");
    let mut presentation = CanvasPresentation::new(canvas.width(), canvas.height());
    let atlas = mix::snow_atlas();
    for case in [2, 3, 7, 8] {
        let layers = mix::snow_scene(case)
            .into_iter()
            .map(WorldLayer::Surface)
            .collect::<Vec<_>>();
        canvas_depth::render_canvas_world(
            &canvas,
            &context,
            &atlas,
            &mut presentation,
            &layers,
            mix::camera(case),
            false,
        )
        .unwrap();
        for (probe, [x, y]) in mix::PROBES.into_iter().enumerate() {
            mix::assert_snow_probe(case, probe, pixel(&presentation.color_buffer, x, y));
        }
        mix::assert_soft(case, &|x, y| pixel(&presentation.color_buffer, x, y));
    }
}

#[wasm_bindgen_test]
fn canvas_shore_blend_matches_across_an_edge_beside_a_third_ground_material() {
    use crate::surface_mesh::{apply_terrain_textures, mix::fixture as shore};
    let mut terrain = shore::shore_terrain();
    // Dirt changes the north quad's palette, but supplies no observation on
    // its shared south edge: that edge is the same sand/water on both sides.
    terrain
        .iter_mut()
        .find(|sample| sample.position == [-1.5, -1.5])
        .unwrap()
        .material = 2;
    let camera = shore::camera(0);
    let mut faces = crate::surface_mesh::projected_surface_triangles(&terrain, camera);
    apply_terrain_textures(&mut faces, &shore::art());
    let mut pair = [[-1, -1], [-1, 0]].map(|tile| {
        *faces
            .iter()
            .find(|face| {
                face.tile == tile
                    && [[-1.0, 0.0, 0.0], [0.0, 0.0, 0.0]]
                        .iter()
                        .all(|world| face.points.iter().any(|point| point.world == *world))
            })
            .expect("two faces meeting on the sand/water edge")
    });
    let edge = aoe_core::Camera {
        center: camera.center,
        zoom: camera.zoom,
        viewport: camera.viewport,
        focus_elevation_meters: camera.focus_elevation_meters,
    }
    .world_to_screen_at_height([-0.1, 0.0], 0.0);
    for face in &mut pair {
        let center = [
            face.points.iter().map(|point| point.screen.x).sum::<f64>() / 3.0,
            face.points.iter().map(|point| point.screen.y).sum::<f64>() / 3.0,
        ];
        // Isolate each face and sample just inside the shared edge, avoiding
        // raster ownership ties while keeping the same world-coordinate probe.
        for point in &mut face.points {
            point.screen.x += 64.5 - edge.x + (edge.x - center[0]) * 0.005;
            point.screen.y += 64.5 - edge.y + (edge.y - center[1]) * 0.005;
        }
    }
    let (canvas, context) = target_canvas().expect("browser Canvas target");
    let mut presentation = CanvasPresentation::new(canvas.width(), canvas.height());
    let atlas = shore::atlas();
    let pixels = pair.map(|face| {
        canvas_depth::render_canvas_world(
            &canvas,
            &context,
            &atlas,
            &mut presentation,
            &[WorldLayer::Surface(face)],
            camera,
            false,
        )
        .unwrap();
        pixel(&presentation.color_buffer, 64, 64)
    });
    shore::junction::assert_edge(&pair, pixels);
}

#[wasm_bindgen_test]
fn canvas_one_level_sand_water_bank_covers_the_review_probe() {
    use crate::surface_mesh::mix::fixture::{self as shore, gap};
    let (canvas, context) = target_canvas().unwrap();
    canvas.set_width(256);
    let mut presentation = CanvasPresentation::new(256, 128);
    for halo in [false, true] {
        let layers = gap::scene(halo)
            .into_iter()
            .map(WorldLayer::Surface)
            .collect::<Vec<_>>();
        canvas_depth::render_canvas_world(
            &canvas,
            &context,
            &shore::atlas(),
            &mut presentation,
            &layers,
            gap::camera(),
            false,
        )
        .unwrap();
        let [x, y] = gap::PROBE;
        let offset = ((y * 256 + x) * 4) as usize;
        gap::assert_pixel(
            presentation.color_buffer[offset..offset + 4]
                .try_into()
                .unwrap(),
            halo,
        );
    }
}

#[wasm_bindgen_test]
fn canvas_building_wear_darkens_native_textured_terrain() {
    use crate::game_renderer::wear_fixture as wear;
    let (canvas, context) = target_canvas().expect("browser Canvas target");
    let mut presentation = CanvasPresentation::new(canvas.width(), canvas.height());
    let atlas = fixture::atlas();
    let mut probes = Vec::new();
    for building in [false, true] {
        let layers = wear::faces(building)
            .into_iter()
            .map(WorldLayer::Surface)
            .collect::<Vec<_>>();
        canvas_depth::render_canvas_world(
            &canvas,
            &context,
            &atlas,
            &mut presentation,
            &layers,
            fixture::camera(0),
            false,
        )
        .unwrap();
        probes.push(fixture::PROBES.map(|[x, y]| pixel(&presentation.color_buffer, x, y)));
    }
    wear::assert_worn(&probes[0], &probes[1]);
}

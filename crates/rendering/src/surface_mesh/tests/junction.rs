//! Four-material junction and isolated shared-edge pixel probes.
use super::{art, color};
use crate::surface_mesh::{
    ProjectedSurfaceTriangle, apply_terrain_textures, projected_surface_triangles,
};
use crate::{SceneCamera, SceneTerrain, SceneTerrainAppearance, SceneTerrainSurface};

pub(crate) fn camera() -> SceneCamera {
    SceneCamera {
        center: [0.5; 2],
        zoom: 1.0,
        viewport: [128.0; 2],
        focus_elevation_meters: 0.0,
    }
}

pub(crate) fn scene() -> Vec<ProjectedSurfaceTriangle> {
    scene_with_bed(false)
}

fn scene_with_bed(forest: bool) -> Vec<ProjectedSurfaceTriangle> {
    let materials = [[8, 9, 9], [4, if forest { 0 } else { 7 }, 9], [4, 4, 8]];
    let terrain = (-1..=1)
        .flat_map(|y| {
            (-1..=1).map(move |x| SceneTerrain {
                position: [f64::from(x) + 0.5, f64::from(y) + 0.5],
                material: materials[(y + 1) as usize][(x + 1) as usize],
                elevation_meters: 0.0,
                surface: SceneTerrainSurface::flat(0.0),
                appearance: Some(SceneTerrainAppearance {
                    floor_strength: if forest && x == 0 && y == 0 { 1000 } else { 0 },
                    canopy_strength: 0,
                    palette: 5,
                    exposure: 0,
                    height_band: 0,
                }),
            })
        })
        .collect::<Vec<_>>();
    let mut faces = projected_surface_triangles(&terrain, camera());
    apply_terrain_textures(&mut faces, &art());
    faces
}

/// Every shared edge, including quad diagonals and boundaries between tiles.
/// Move its midpoint to a pixel centre and just inside each isolated face:
/// raster top-left ownership and GPU subpixel rounding must not turn either
/// readback into background. The inset exceeds half a 1/16-pixel raster step
/// while remaining below 0.1 pixel in this fixture; tolerances stay at two bytes.
pub(crate) fn edge_pairs() -> Vec<[ProjectedSurfaceTriangle; 2]> {
    [false, true].into_iter().flat_map(pairs_with_bed).collect()
}

fn pairs_with_bed(forest: bool) -> Vec<[ProjectedSurfaceTriangle; 2]> {
    let faces = scene_with_bed(forest);
    let mut pairs = Vec::new();
    for (index, a) in faces.iter().enumerate() {
        for b in &faces[index + 1..] {
            let shared = a
                .points
                .iter()
                .filter(|p| b.points.iter().any(|q| p.world == q.world))
                .collect::<Vec<_>>();
            if shared.len() != 2 {
                continue;
            }
            let middle = [
                (shared[0].screen.x + shared[1].screen.x) * 0.5,
                (shared[0].screen.y + shared[1].screen.y) * 0.5,
            ];
            let mut pair = [*a, *b];
            for face in &mut pair {
                let center = [
                    face.points.iter().map(|p| p.screen.x).sum::<f64>() / 3.0,
                    face.points.iter().map(|p| p.screen.y).sum::<f64>() / 3.0,
                ];
                for point in &mut face.points {
                    point.screen.x += 64.5 - middle[0] + (middle[0] - center[0]) * 0.005;
                    point.screen.y += 64.5 - middle[1] + (middle[1] - center[1]) * 0.005;
                }
            }
            pairs.push(pair);
        }
    }
    assert!(pairs.len() > 9, "exercise neighbouring tiles too");
    pairs
}

pub(crate) fn assert_edge(pair: &[ProjectedSurfaceTriangle; 2], pixels: [[u8; 4]; 2]) {
    assert_eq!(pixels[0][3], 255, "first face covers the edge probe");
    assert_eq!(pixels[1][3], 255, "second face covers the edge probe");
    for channel in 0..4 {
        assert!(
            pixels[0][channel].abs_diff(pixels[1][channel]) <= 2,
            "edge {:?}/{:?}: {pixels:?}; packets {:?}",
            pair[0].tile,
            pair[1].tile,
            pair.map(|face| {
                let packet = crate::web::surface_instance(&face, [128.0; 2], 0.0);
                (packet.terrain_blend, packet.pages, packet.depths)
            })
        );
    }
    if pair.iter().all(|face| face.tile == [0, 0]) {
        // Independent four-layer cumulative thresholds at [0.5, 0.5].
        let noise = super::super::noise([0, 0], [0.5; 2]);
        let edge = |s: f32| ((s - 0.5) * 4.0 + 0.5 + (noise - 0.5) * 2.6).clamp(0.0, 1.0);
        let thresholds = [
            1.0,
            edge(192.0 / 255.0),
            edge(128.0 / 255.0),
            edge(64.0 / 255.0),
            0.0,
        ];
        let forest = pair[0].material == 0;
        let colors = [8, 4, 9, if forest { 0 } else { 7 }].map(color);
        let bed = if forest { 64.0 / 255.0 } else { 0.0 };
        let litter = super::layer(super::COLORS[6], 6);
        for pixel in pixels {
            for channel in 0..4 {
                let expected = (0..4)
                    .map(|i| colors[i][channel] * (thresholds[i] - thresholds[i + 1]))
                    .sum::<f32>();
                let expected = (expected * (1.0 - bed) + litter[channel] * bed).round() as u8;
                assert!(
                    pixel[channel].abs_diff(expected) <= 2,
                    "junction: {pixel:?}, channel {channel} expected {expected}"
                );
            }
        }
    }
}

/// Colour reconstructed independently from the mesh's encoded observations at
/// the diagonal midpoint. The review fixture has four equal corner shares.
pub(crate) fn diagonal_colour(face: &ProjectedSurfaceTriangle) -> [u8; 4] {
    let observations = face.texture_blend.expect("junction observations")[1].uv;
    let endpoints = [[0.0, 0.0, 0.0], [1.0, 1.0, 0.0]].map(|world| {
        let corner = face
            .points
            .iter()
            .position(|point| point.world == world)
            .unwrap();
        let word = observations[corner] as u16;
        let mut materials = (0..4)
            .map(|slot| ((word >> (slot * 4)) & 15) as u8)
            .collect::<Vec<_>>();
        materials.sort_unstable();
        assert_eq!(
            materials,
            [4, 7, 8, 9],
            "four observations on the shared diagonal"
        );
        word
    });
    let noise = super::super::noise([0, 0], [0.5; 2]);
    let mut result = [0.0; 4];
    let (mut cumulative, mut previous) = (0.0, 0.0);
    for material in [7, 9, 4, 8] {
        let share = endpoints.map(|word| {
            let matching = (0..4)
                .filter(|slot| ((word >> (slot * 4)) & 15) as u8 == material)
                .count();
            ((matching * 255 + 2) / 4) as f32 / 255.0
        });
        cumulative += (share[0] + share[1]) * 0.5;
        let threshold = ((cumulative - 0.5) * 4.0 + 0.5 + (noise - 0.5) * 2.6).clamp(0.0, 1.0);
        let layer = color(material);
        for channel in 0..4 {
            result[channel] += layer[channel] * (threshold - previous);
        }
        previous = threshold;
    }
    result.map(|value| value.round() as u8)
}

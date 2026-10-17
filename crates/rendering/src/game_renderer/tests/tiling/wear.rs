//! Native-art terrain with and without a Town Center, shared by the three
//! renderer pixel tests: footprint wear must reach textured ground, not only
//! the untextured fallback colour.
use super::*;
use crate::surface_mesh::ProjectedSurfaceTriangle;
use crate::surface_mesh::directory::fixture;

/// The directory-fixture grass scene (near zoom), after the production ground pass.
pub(crate) fn faces(building: bool) -> Vec<ProjectedSurfaceTriangle> {
    let camera = fixture::camera(0);
    let town_center = SceneUnit {
        kind: crate::SceneUnitKind::TownCenter,
        id: aoe_core::EntityId(2),
        position: camera.center,
        moving: false,
        facing: 0,
        selected: false,
        elevation_meters: 0.0,
    };
    let units = if building {
        vec![town_center]
    } else {
        Vec::new()
    };
    direct_world_layers(
        fixture::scene(0),
        &fixture::art(),
        &[],
        &[],
        &units,
        camera,
        0,
    )
    .into_iter()
    .filter_map(|layer| match layer {
        WorldLayer::Surface(face) => Some(face),
        _ => None,
    })
    .collect()
}

/// Every probe sits inside the full-strength footprint: visibly darker, never black.
pub(crate) fn assert_worn(plain: &[[u8; 4]], worn: &[[u8; 4]]) {
    assert_eq!(plain.len(), worn.len());
    for (index, (plain, worn)) in plain.iter().zip(worn).enumerate() {
        let sum = |pixel: &[u8; 4]| pixel[..3].iter().map(|v| u32::from(*v)).sum::<u32>();
        assert_eq!(plain[3], worn[3], "probe {index} alpha");
        assert!(sum(plain) > 0, "probe {index} samples native art");
        assert!(
            sum(worn) * 100 <= sum(plain) * 90,
            "probe {index}: textured ground must darken under a building: {plain:?} -> {worn:?}"
        );
        assert!(
            sum(worn) * 100 >= sum(plain) * 50,
            "probe {index}: wear stays natural, not black: {plain:?} -> {worn:?}"
        );
    }
}

use super::*;

pub(super) fn direct_world_layers(
    surfaces: impl IntoIterator<Item = ProjectedSurfaceTriangle>,
    art: &GameArt,
    terrain: &[SceneTerrain],
    resources: &[SceneResource],
    units: &[SceneUnit],
    camera: SceneCamera,
    animation: usize,
) -> Vec<WorldLayer> {
    let ground = ground::BuildingGround::new(units, camera);
    let compatibility = if terrain.is_empty() {
        ground.compatibility_farms(camera)
    } else {
        Vec::new()
    };
    let surfaces = surfaces.into_iter().chain(compatibility).map(|mut face| {
        ground.apply(&mut face);
        ground.farm(&mut face, art);
        face
    });
    // Reserve the ordinary shadow/body pairs and the extra Town Center pieces.
    let object_capacity = resources
        .len()
        .saturating_add(units.len())
        .saturating_mul(2)
        .saturating_add(
            units
                .iter()
                .filter(|u| u.kind == SceneUnitKind::TownCenter)
                .count()
                * 6,
        );
    ordered_world_layers_with(surfaces, object_capacity, units, camera, |entries| {
        world_sprites::emit_world_sprite_frames(
            art,
            terrain,
            resources,
            units,
            camera,
            animation,
            |(sprite, frame, depth, id)| entries.push(WorldLayer::Sprite(sprite, frame, depth, id)),
        );
    })
}

#[cfg(test)]
pub(super) fn ordered_world_layers(
    surfaces: impl IntoIterator<Item = ProjectedSurfaceTriangle>,
    objects: Vec<(Sprite, GameFrame, f64, u64)>,
    units: &[SceneUnit],
    camera: SceneCamera,
) -> Vec<WorldLayer> {
    ordered_world_layers_with(surfaces, objects.len(), units, camera, |entries| {
        entries.extend(
            objects
                .into_iter()
                .map(|(sprite, frame, depth, id)| WorldLayer::Sprite(sprite, frame, depth, id)),
        );
    })
}

fn ordered_world_layers_with(
    surfaces: impl IntoIterator<Item = ProjectedSurfaceTriangle>,
    object_capacity: usize,
    units: &[SceneUnit],
    camera: SceneCamera,
    emit: impl FnOnce(&mut Vec<WorldLayer>),
) -> Vec<WorldLayer> {
    let surfaces = surfaces.into_iter();
    let selected_count = units.iter().filter(|unit| unit.selected).count();
    let mut entries = Vec::with_capacity(
        surfaces
            .size_hint()
            .0
            .saturating_add(object_capacity)
            .saturating_add(selected_count.saturating_mul(game_grid::SELECTION_RING_SPRITES)),
    );
    entries.extend(surfaces.map(WorldLayer::Surface));
    for unit in units.iter().filter(|unit| unit.selected) {
        entries.extend(
            (if let Some(side) = unit.kind.building_side() {
                game_grid::selection_ellipse(
                    camera,
                    unit.position,
                    unit.elevation_meters,
                    [
                        side / 2.0 * aoe_core::ISO_TILE_WIDTH,
                        side / 2.0 * aoe_core::ISO_TILE_HEIGHT,
                    ],
                )
            } else {
                game_grid::selection_ring(camera, unit.position, unit.elevation_meters)
            })
            .into_iter()
            .map(|(sprite, depth)| WorldLayer::Selection(sprite, depth)),
        );
    }
    emit(&mut entries);
    // Compose the former object (depth, id) pre-sort with layer (depth, kind)
    // ordering in one stable sort. Exact id ties keep shadow/body and source
    // input order, without a second scene or a second object sort buffer.
    entries.sort_by(|left, right| {
        let left = layer_order(left);
        let right = layer_order(right);
        left.0
            .total_cmp(&right.0)
            .then(left.1.cmp(&right.1))
            .then(left.2.cmp(&right.2))
    });
    entries
}

pub(super) fn layer_order(layer: &WorldLayer) -> (f64, u8, u64) {
    match layer {
        WorldLayer::Surface(triangle) => (triangle_depth(triangle), 0, 0),
        WorldLayer::Selection(_, depth) => (*depth, 1, 0),
        WorldLayer::Sprite(_, _, depth, id) => (*depth, 2, *id),
    }
}

pub(super) fn triangle_depth(triangle: &ProjectedSurfaceTriangle) -> f64 {
    triangle
        .points
        .iter()
        .map(|point| surface_render_depth(point.world, triangle.skirt))
        .sum::<f64>()
        / 3.0
}

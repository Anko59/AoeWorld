use super::*;

#[inline(never)]
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
    let surfaces = surfaces.into_iter().chain(compatibility);
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
    let mut entries = Vec::with_capacity(capacity(surfaces.size_hint().0, object_capacity, units));
    for mut face in surfaces {
        ground.apply(&mut face);
        ground.farm(&mut face, art);
        entries.push(WorldLayer::Surface(face));
    }
    append_selections(&mut entries, units, camera);
    world_sprites::emit_world_sprite_frames(
        art,
        terrain,
        resources,
        units,
        camera,
        animation,
        |(sprite, frame, depth, id)| entries.push(WorldLayer::Sprite(sprite, frame, depth, id)),
    );
    sort_layers(&mut entries);
    entries
}

#[cfg(test)]
pub(super) fn ordered_world_layers(
    surfaces: impl IntoIterator<Item = ProjectedSurfaceTriangle>,
    objects: Vec<(Sprite, GameFrame, f64, u64)>,
    units: &[SceneUnit],
    camera: SceneCamera,
) -> Vec<WorldLayer> {
    let surfaces = surfaces.into_iter();
    let mut entries = Vec::with_capacity(capacity(surfaces.size_hint().0, objects.len(), units));
    for surface in surfaces {
        entries.push(WorldLayer::Surface(surface));
    }
    append_selections(&mut entries, units, camera);
    for (sprite, frame, depth, id) in objects {
        entries.push(WorldLayer::Sprite(sprite, frame, depth, id));
    }
    sort_layers(&mut entries);
    entries
}

fn capacity(surfaces: usize, objects: usize, units: &[SceneUnit]) -> usize {
    surfaces.saturating_add(objects).saturating_add(
        units
            .iter()
            .filter(|unit| unit.visual.selected())
            .count()
            .saturating_mul(game_grid::SELECTION_RING_SPRITES),
    )
}

// Direct pushes keep one shared layer builder instead of iterator adapter
// specializations for each terrain source and sprite callback in WASM.
#[inline(never)]
fn append_selections(entries: &mut Vec<WorldLayer>, units: &[SceneUnit], camera: SceneCamera) {
    for unit in units.iter().filter(|unit| unit.visual.selected()) {
        let selections = if let Some(side) = unit.kind.building_side() {
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
        };
        for (mut sprite, depth) in selections {
            if unit.visual.hostile() {
                sprite.color = [245.0 / 255.0, 65.0 / 255.0, 65.0 / 255.0, 1.0];
            }
            entries.push(WorldLayer::Selection(sprite, depth));
        }
    }
}

#[inline(never)]
fn sort_layers(entries: &mut [WorldLayer]) {
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

use super::*;

impl GameRenderer {
    pub(super) fn world_layers(
        &mut self,
        surfaces: impl IntoIterator<Item = ProjectedSurfaceTriangle>,
        art: &GameArt,
        terrain: &[SceneTerrain],
        resources: &[SceneResource],
        units: &[SceneUnit],
        camera: SceneCamera,
        animation: usize,
    ) -> Vec<WorldLayer> {
        let sort = match self {
            Self::WebGpu(renderer) => &mut renderer.layer_sort,
            Self::WebGl(renderer) => &mut renderer.layer_sort,
            Self::Canvas { presentation, .. } => &mut presentation.layer_sort,
        };
        build_layers(
            surfaces, art, terrain, resources, units, camera, animation, sort,
        )
    }
}

#[cfg(test)]
pub(super) fn direct_world_layers(
    surfaces: impl IntoIterator<Item = ProjectedSurfaceTriangle>,
    art: &GameArt,
    terrain: &[SceneTerrain],
    resources: &[SceneResource],
    units: &[SceneUnit],
    camera: SceneCamera,
    animation: usize,
) -> Vec<WorldLayer> {
    build_layers(
        surfaces,
        art,
        terrain,
        resources,
        units,
        camera,
        animation,
        &mut LayerSort::default(),
    )
}

#[inline(never)]
fn build_layers(
    surfaces: impl IntoIterator<Item = ProjectedSurfaceTriangle>,
    art: &GameArt,
    terrain: &[SceneTerrain],
    resources: &[SceneResource],
    units: &[SceneUnit],
    camera: SceneCamera,
    animation: usize,
    sort: &mut LayerSort,
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
    let terrain_span = perf::Stage::new(3);
    for mut face in surfaces {
        ground.apply(&mut face);
        ground.farm(&mut face, art);
        entries.push(WorldLayer::Surface(face));
    }
    drop(terrain_span);
    let sprites_span = perf::Stage::new(4);
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
    sort.sort(&mut entries);
    drop(sprites_span);
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
    LayerSort::default().sort(&mut entries);
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
        let [(mut sprite, depth)] = if let Some(side) = unit.kind.building_side() {
            game_grid::selection_ellipse(
                camera,
                unit.position,
                f64::from(unit.elevation_meters),
                [
                    side / 2.0 * aoe_core::ISO_TILE_WIDTH,
                    side / 2.0 * aoe_core::ISO_TILE_HEIGHT,
                ],
            )
        } else {
            game_grid::selection_ring(camera, unit.position, f64::from(unit.elevation_meters))
        };
        if unit.visual.hostile() {
            sprite.color = [245.0 / 255.0, 65.0 / 255.0, 65.0 / 255.0, 1.0];
        }
        entries.push(WorldLayer::Selection(sprite, depth));
    }
}

#[derive(Default)]
pub(crate) struct LayerSort {
    keys: Vec<LayerKey>,
}

struct LayerKey {
    depth: f64,
    id: u64,
    source: usize,
    kind: u8,
}

impl LayerSort {
    pub(crate) fn resident_bytes(&self) -> usize {
        self.keys.capacity() * std::mem::size_of::<LayerKey>()
    }

    #[inline(never)]
    fn sort(&mut self, entries: &mut [WorldLayer]) {
        let keys = &mut self.keys;
        keys.clear();
        keys.extend(entries.iter().enumerate().map(|(source, layer)| {
            let (depth, kind, id) = layer_order(layer);
            LayerKey {
                depth,
                id,
                source,
                kind,
            }
        }));
        // Sort the small keys in place. The original ordinal keeps exact ties
        // stable, including shadow/body and source order.
        keys.sort_unstable_by(|left, right| {
            left.depth
                .total_cmp(&right.depth)
                .then(left.kind.cmp(&right.kind))
                .then(left.id.cmp(&right.id))
                .then(left.source.cmp(&right.source))
        });
        // Each sorted key maps a destination to its original source. Move
        // one cycle at a time and mark its finished destinations in the keys.
        for start in 0..keys.len() {
            if keys[start].source == start {
                continue;
            }
            let saved = entries[start];
            let mut destination = start;
            while keys[destination].source != start {
                let source = std::mem::replace(&mut keys[destination].source, destination);
                entries[destination] = entries[source];
                destination = source;
            }
            entries[destination] = saved;
            keys[destination].source = destination;
        }
    }
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

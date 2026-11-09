//! The ground of a scene as draw-ready packets, kept between frames.
//!
//! Objects move every frame; the ground under a still camera does not. Its
//! packets depend on the prepared scene, the art's residency, the fog, the
//! buildings that wear or plough it, the camera and the grid switch. `Key`
//! names all of them: a frame with an equal key copies the kept packets and
//! builds only its selections and sprites. Canvas rasterizes the triangles
//! themselves and keeps its own damage record, so it takes no part.
use super::world_layers::{layer_order, object_layers, terrain_layers};
use super::*;
use crate::web::DepthRange;

/// What only the caller can tell apart: which prepared scene the surfaces
/// are, and how often the art's residency and the fog have changed.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct TerrainKey {
    pub scene: u64,
    pub art: u64,
    pub fog: u64,
}

/// Every input of the ground packets and of the grid lines over them.
#[derive(PartialEq)]
struct Key {
    inputs: TerrainKey,
    camera: SceneCamera,
    ground: ground::BuildingGround,
    grid: bool,
}

/// Ground packets in drawing order with their sort depths, and the grid lines.
#[derive(Default)]
pub struct TerrainCache {
    key: Option<Key>,
    packets: Vec<Sprite>,
    /// The layer depth each packet is sorted by, ascending.
    order: Vec<f64>,
    /// Camera-relative depths of each packet, before normalization.
    raw: Vec<[f32; 4]>,
    grid: Vec<Sprite>,
    /// The ground's own depth range, fixed with the key.
    depths: DepthRange,
    /// The range `packets` are normalized with; `None` while they hold `raw`.
    normalized: Option<Option<(f32, f32)>>,
    built: usize,
}

impl TerrainCache {
    /// Ground packets the last frame had to build: zero when it reused them.
    pub fn built(&self) -> usize {
        self.built
    }

    /// Owned payload of the kept packets.
    pub fn resident_bytes(&self) -> usize {
        (self.packets.capacity() + self.grid.capacity()) * size_of::<Sprite>()
            + self.order.capacity() * size_of::<f64>()
            + self.raw.capacity() * size_of::<[f32; 4]>()
            + self
                .key
                .as_ref()
                .map_or(0, |key| key.ground.resident_bytes())
    }

    fn rebuild(
        &mut self,
        key: Key,
        art: &GameArt,
        terrain: &[SceneTerrain],
        surfaces: &[ProjectedSurfaceTriangle],
        origin: f64,
        light: &dyn Fn([f64; 2]) -> f32,
    ) {
        let camera = key.camera;
        let layers = terrain_layers(surfaces, art, terrain, &key.ground, camera);
        self.packets.clear();
        self.packets.reserve(layers.len());
        self.order.clear();
        self.order.reserve(layers.len());
        self.raw.clear();
        self.raw.reserve(layers.len());
        self.depths = DepthRange::default();
        self.normalized = None;
        for layer in &layers {
            let WorldLayer::Surface(triangle) = layer else {
                continue;
            };
            let mut packet = crate::web::surface_instance(triangle, camera.viewport, origin);
            fog::surface_packet(&mut packet, fog::vertices(triangle, light));
            self.depths.include(&packet);
            self.order.push(layer_order(layer).0);
            self.raw.push(packet.depths);
            self.packets.push(packet);
        }
        self.grid = if key.grid {
            let mut lines = game_grid::grid_sprites(
                &fog::grid_segments(&layers, camera, light),
                camera.viewport,
            );
            // Overlay depths are not finite: the front, whatever the range.
            for line in &mut lines {
                DepthRange::default().normalize(line);
            }
            lines
        } else {
            Vec::new()
        };
        self.built = self.packets.len();
        self.key = Some(key);
    }

    /// Leaves the kept packets normalized with `range`. The ground's own range
    /// is fixed with the key, so a frame whose objects all lie inside it finds
    /// the depths of the frame before. An object beyond it widens the frame's
    /// range, as the whole-frame pass always did: the depths are then derived
    /// again from the raw ones, without building a packet.
    fn normalize(&mut self, range: DepthRange) {
        if self.normalized == Some(range.range) {
            return;
        }
        for (packet, raw) in self.packets.iter_mut().zip(&self.raw) {
            packet.depths = *raw;
            range.normalize(packet);
        }
        self.normalized = Some(range.range);
    }

    /// Fills `instances` with the frame in drawing order, depths normalized,
    /// and returns whether it holds hidden units to outline. `objects` are the
    /// frame's selections and sprites, sorted as the ground is.
    fn frame(
        &mut self,
        instances: &mut Vec<Sprite>,
        objects: &mut [WorldLayer],
        origin: f64,
    ) -> bool {
        let mut range = self.depths;
        for layer in objects.iter_mut() {
            let (sprite, depths) = match layer {
                WorldLayer::Selection(sprite, depth) => {
                    let depths = game_grid::selection_instance_depths(sprite, *depth, origin);
                    (sprite, depths)
                }
                WorldLayer::Sprite(sprite, _, depth, _) => {
                    let depths = [(*depth - origin) as f32; 4];
                    (sprite, depths)
                }
                WorldLayer::Surface(_) => continue,
            };
            sprite.depths = depths;
            range.include(sprite);
        }
        self.normalize(range);
        instances.clear();
        instances.reserve(self.packets.len() + objects.len() + self.grid.len());
        let mut next = 0;
        for layer in objects.iter() {
            let (WorldLayer::Selection(sprite, _) | WorldLayer::Sprite(sprite, ..)) = layer else {
                continue;
            };
            // One stable sort of ground, rings and sprites by (depth, kind, id)
            // puts the ground of an object's depth before it.
            let depth = layer_order(layer).0;
            let run = self.order[next..]
                .iter()
                .take_while(|key| key.total_cmp(&depth).is_le())
                .count();
            instances.extend_from_slice(&self.packets[next..next + run]);
            next += run;
            let mut sprite = *sprite;
            range.normalize(&mut sprite);
            instances.push(sprite);
        }
        instances.extend_from_slice(&self.packets[next..]);
        instances.extend_from_slice(&self.grid);
        range.effects()
    }
}

impl GameRenderer {
    /// `render_prepared_world_with_fog` with the ground's packets kept in
    /// `cache` between frames. `key` must change whenever `surfaces`, the
    /// art's residency or what `light` answers does; the camera, the
    /// building ground and the grid switch are compared here.
    pub fn render_cached_world(
        &mut self,
        cache: &mut TerrainCache,
        key: TerrainKey,
        art: &GameArt,
        terrain: &[SceneTerrain],
        surfaces: &[ProjectedSurfaceTriangle],
        resources: &[SceneResource],
        units: &[SceneUnit],
        camera: SceneCamera,
        animation: usize,
        grid: bool,
        fog_active: bool,
        light: &dyn Fn([f64; 2]) -> f32,
    ) -> Result<bool, String> {
        cache.built = 0;
        let mut instances = match self {
            Self::WebGpu(renderer) => std::mem::take(&mut renderer.staging),
            Self::WebGl(_) => Vec::new(),
            Self::Canvas {
                canvas,
                context,
                source_atlas,
                presentation,
                ..
            } => {
                let surfaces = surfaces.iter().copied().map(|mut triangle| {
                    apply_terrain_textures(std::slice::from_mut(&mut triangle), art);
                    triangle
                });
                let mut layers = direct_world_layers(
                    surfaces, art, terrain, resources, units, camera, animation,
                );
                fog::shade_objects(&mut layers, art, resources, units, animation, light);
                if canvas.width() == 0 || canvas.height() == 0 {
                    return Ok(false);
                }
                render_canvas_world_with_fog(
                    canvas,
                    context,
                    source_atlas,
                    presentation,
                    &layers,
                    camera,
                    grid,
                    fog_active,
                    light,
                )?;
                return Ok(true);
            }
        };
        let origin = surface_depth([
            camera.center[0],
            camera.center[1],
            camera.focus_elevation_meters,
        ]);
        let key = Key {
            inputs: key,
            camera,
            ground: ground::BuildingGround::new(units, camera),
            grid,
        };
        if cache.key.as_ref() != Some(&key) {
            cache.rebuild(key, art, terrain, surfaces, origin, light);
        }
        let mut objects = object_layers(art, terrain, resources, units, camera, animation);
        fog::shade_objects(&mut objects, art, resources, units, animation, light);
        let effects = Some(cache.frame(&mut instances, &mut objects, origin));
        match self {
            Self::WebGpu(renderer) => {
                let clear = if fog_active {
                    [0.0, 0.0, 0.0, 1.0]
                } else {
                    [0.16, 0.29, 0.14, 1.0]
                };
                let result = renderer
                    .submit_instances(&mut instances, effects, clear)
                    .map(|counters| counters.did_present);
                renderer.staging = instances;
                result
            }
            Self::WebGl(renderer) => renderer.submit(&mut instances, fog_active, effects),
            // Drawn and returned above.
            Self::Canvas { .. } => Ok(false),
        }
    }
}

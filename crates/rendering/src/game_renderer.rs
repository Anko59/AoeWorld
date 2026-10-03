//! WebGPU-first game rendering with a Canvas 2D compatibility path.
use crate::{
    GAME_ATLAS_SIDE, GameArt, GameFrame, Renderer, game_grid,
    playground::game_sprites,
    surface_mesh::{
        ProjectedSurfaceTriangle, apply_terrain_textures, projected_surface_triangles,
        surface_depth, surface_render_depth,
    },
    web::Sprite,
};
use aoe_core::{Camera, EntityId};
use wasm_bindgen::{Clamped, JsCast, JsValue};
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, ImageData};

#[path = "game_renderer/world_sprites.rs"]
mod world_sprites;
use world_sprites::world_sprite_frames;

pub fn resource_sprite_bounds(
    resource: SceneResource,
    frame: GameFrame,
    camera: SceneCamera,
) -> Option<[f64; 4]> {
    world_sprites::resource_sprite_bounds(resource, frame, camera)
}

#[path = "game_renderer/canvas_depth.rs"]
mod canvas_depth;
use canvas_depth::{CanvasPresentation, render_canvas_world};

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "game_renderer/tests/canvas_depth.rs"]
mod canvas_depth_tests;

pub enum GameRenderer {
    WebGpu(Box<Renderer>),
    Canvas {
        canvas: HtmlCanvasElement,
        context: CanvasRenderingContext2d,
        atlas: HtmlCanvasElement,
        source_atlas: Vec<u8>,
        presentation: CanvasPresentation,
    },
}
#[derive(Clone, Copy)]
pub struct SceneCamera {
    pub center: [f64; 2],
    pub zoom: f64,
    pub viewport: [f64; 2],
    pub focus_elevation_meters: f64,
}

#[derive(Clone, Copy, PartialEq)]
pub struct SceneUnit {
    pub id: EntityId,
    pub position: [f64; 2],
    pub moving: bool,
    pub facing: u8,
    pub selected: bool,
    pub elevation_meters: f64,
}

#[derive(Clone, Copy)]
pub struct SceneTerrain {
    pub position: [f64; 2],
    /// One of the six `GameArt::terrain` groups.
    pub material: u8,
    pub elevation_meters: f64,
    pub surface: SceneTerrainSurface,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SceneTerrainSurface {
    /// Shared corners ordered northwest, northeast, southeast, southwest.
    pub corner_game_height_levels: [i16; 4],
    /// aoe-map `SurfaceKind` discriminant: plateau, ramp, cliff.
    pub kind: u8,
    /// aoe-map `SurfaceDiagonal` discriminant.
    pub triangulation: u8,
    /// aoe-map `WaterKind` discriminant; zero means no water.
    pub water: u8,
}

impl SceneTerrainSurface {
    pub const PLATEAU: u8 = 0;
    pub const RAMP: u8 = 1;
    pub const CLIFF: u8 = 2;

    pub const fn flat(elevation_meters: f64) -> Self {
        Self {
            corner_game_height_levels: [elevation_meters as i16; 4],
            kind: Self::PLATEAU,
            triangulation: 0,
            water: 0,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct SceneResource {
    pub id: u64,
    pub position: [f64; 2],
    /// Resource kind in the versioned map wire order.
    pub kind: u8,
    pub visual_variant: u8,
    pub elevation_meters: f64,
}
fn error(e: impl Into<JsValue>) -> String {
    format!("Canvas rendering unavailable: {:?}", e.into())
}

fn context(canvas: &HtmlCanvasElement) -> Result<CanvasRenderingContext2d, String> {
    canvas
        .get_context("2d")
        .map_err(error)?
        .ok_or("Canvas 2D is unavailable")?
        .dyn_into()
        .map_err(error)
}

impl GameRenderer {
    pub async fn new(canvas: HtmlCanvasElement) -> Result<(Self, HtmlCanvasElement), String> {
        if let Ok(renderer) = Renderer::new(canvas.clone()).await {
            return Ok((Self::WebGpu(Box::new(renderer)), canvas));
        }
        // A failed WebGPU attempt may still bind the context type. Replace the
        // element before controls are installed, retaining all attributes.
        let replacement: HtmlCanvasElement = canvas
            .clone_node()
            .map_err(error)?
            .dyn_into()
            .map_err(error)?;
        canvas
            .parent_node()
            .ok_or("Canvas is detached")?
            .replace_child(&replacement, &canvas)
            .map_err(error)?;
        let main_context = context(&replacement)?;
        Ok((
            Self::Canvas {
                canvas: replacement.clone(),
                context: main_context,
                atlas: new_atlas(&replacement)?,
                source_atlas: Vec::new(),
                presentation: CanvasPresentation::new(replacement.width(), replacement.height()),
            },
            replacement,
        ))
    }

    pub fn backend(&self) -> &'static str {
        match self {
            Self::WebGpu(_) => "webgpu",
            Self::Canvas { .. } => "canvas2d",
        }
    }

    pub fn upload_game_atlas(&mut self, pixels: &[u8]) -> Result<(), String> {
        match self {
            Self::WebGpu(renderer) => renderer.upload_game_atlas(pixels),
            Self::Canvas {
                atlas,
                source_atlas,
                ..
            } => {
                if pixels.len() != (GAME_ATLAS_SIDE * GAME_ATLAS_SIDE * 4) as usize {
                    return Err("Invalid game atlas size".into());
                }
                let data = ImageData::new_with_u8_clamped_array_and_sh(
                    Clamped(pixels),
                    GAME_ATLAS_SIDE,
                    GAME_ATLAS_SIDE,
                )
                .map_err(error)?;
                context(atlas)?
                    .put_image_data(&data, 0.0, 0.0)
                    .map_err(error)?;
                *source_atlas = pixels.to_vec();
                Ok(())
            }
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        match self {
            Self::WebGpu(renderer) => renderer.resize(width, height),
            Self::Canvas { presentation, .. } => presentation.resize(width, height),
        }
    }

    pub fn render_game(
        &mut self,
        art: &GameArt,
        unit: [f32; 2],
        target: [f32; 2],
        moving: bool,
        animation: usize,
        facing: (usize, bool),
    ) -> Result<(), String> {
        match self {
            Self::WebGpu(renderer) => renderer
                .render_game(art, unit, target, moving, animation, facing)
                .map(|_| ()),
            Self::Canvas {
                canvas,
                context,
                atlas,
                ..
            } => {
                let width = f64::from(canvas.width());
                let height = f64::from(canvas.height());
                context.clear_rect(0.0, 0.0, width, height);
                context.set_image_smoothing_enabled(false);
                for sprite in game_sprites(art, unit, target, moving, animation, facing) {
                    let x = f64::from(sprite.position[0] - sprite.radius[0] + 1.0) * width / 2.0;
                    let y = f64::from(1.0 - sprite.position[1] - sprite.radius[1]) * height / 2.0;
                    let w = f64::from(sprite.radius[0]) * width;
                    let h = f64::from(sprite.radius[1]) * height;
                    if sprite.color != [1.0; 4] {
                        let [r, g, b, a] = sprite.color;
                        context.set_fill_style_str(&format!(
                            "rgba({},{},{},{a})",
                            (r * 255.0) as u8,
                            (g * 255.0) as u8,
                            (b * 255.0) as u8
                        ));
                        context.fill_rect(x, y, w, h);
                        continue;
                    }
                    let [mut sx, sy, sw, sh] =
                        sprite.uv.map(|n| f64::from(n) * f64::from(GAME_ATLAS_SIDE));
                    context.save();
                    if sw < 0.0 {
                        sx += sw;
                        context.translate(x + w, y).map_err(error)?;
                        context.scale(-1.0, 1.0).map_err(error)?;
                    } else {
                        context.translate(x, y).map_err(error)?;
                    }
                    let result = context.draw_image_with_html_canvas_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(atlas, sx, sy, sw.abs(), sh, 0.0, 0.0, w, h).map_err(error);
                    context.restore();
                    result?;
                }
                Ok(())
            }
        }
    }

    pub fn render_world(
        &mut self,
        art: &GameArt,
        terrain: &[SceneTerrain],
        resources: &[SceneResource],
        units: &[SceneUnit],
        camera: SceneCamera,
        animation: usize,
        grid: bool,
    ) -> Result<(), String> {
        let surfaces = projected_surface_triangles(terrain, camera);
        self.render_prepared_world(
            art, terrain, &surfaces, resources, units, camera, animation, grid,
        )
    }

    pub fn render_prepared_world(
        &mut self,
        art: &GameArt,
        terrain: &[SceneTerrain],
        surfaces: &[ProjectedSurfaceTriangle],
        resources: &[SceneResource],
        units: &[SceneUnit],
        camera: SceneCamera,
        animation: usize,
        grid: bool,
    ) -> Result<(), String> {
        let surfaces = surfaces.iter().copied().map(|mut triangle| {
            apply_terrain_textures(std::slice::from_mut(&mut triangle), art);
            triangle
        });
        let object_sprites = world_sprite_frames(art, terrain, resources, units, camera, animation);
        let layers = ordered_world_layers(surfaces, object_sprites, units, camera);
        match self {
            Self::WebGpu(renderer) => {
                let depth_origin = surface_depth([
                    camera.center[0],
                    camera.center[1],
                    camera.focus_elevation_meters,
                ]);
                let mut instances = layers
                    .iter()
                    .map(|layer| match layer {
                        WorldLayer::Surface(triangle) => {
                            crate::web::surface_instance(triangle, camera.viewport, depth_origin)
                        }
                        WorldLayer::Selection(sprite, depth)
                        | WorldLayer::Sprite(sprite, _, depth, _) => {
                            let mut sprite = *sprite;
                            sprite.depths = [(*depth - depth_origin) as f32; 4];
                            sprite
                        }
                    })
                    .collect::<Vec<_>>();
                if grid {
                    instances.extend(game_grid::grid_sprites(camera));
                }
                renderer
                    .render_sprites_with_clear(&instances, [0.16, 0.29, 0.14, 1.0])
                    .map(|_| ())
            }
            Self::Canvas {
                canvas,
                context,
                source_atlas,
                presentation,
                ..
            } => render_canvas_world(
                canvas,
                context,
                source_atlas,
                presentation,
                &layers,
                camera,
                grid,
            ),
        }
    }
}

fn new_atlas(canvas: &HtmlCanvasElement) -> Result<HtmlCanvasElement, String> {
    let atlas: HtmlCanvasElement = canvas
        .owner_document()
        .ok_or("No document")?
        .create_element("canvas")
        .map_err(error)?
        .dyn_into()
        .map_err(error)?;
    atlas.set_width(GAME_ATLAS_SIDE);
    atlas.set_height(GAME_ATLAS_SIDE);
    Ok(atlas)
}

fn ordered_world_layers(
    surfaces: impl IntoIterator<Item = ProjectedSurfaceTriangle>,
    objects: Vec<(Sprite, GameFrame, f64, u64)>,
    units: &[SceneUnit],
    camera: SceneCamera,
) -> Vec<WorldLayer> {
    let surfaces = surfaces.into_iter();
    let selected_count = units.iter().filter(|unit| unit.selected).count();
    let mut entries = Vec::with_capacity(
        surfaces
            .size_hint()
            .0
            .saturating_add(objects.len())
            .saturating_add(selected_count.saturating_mul(game_grid::SELECTION_RING_SPRITES)),
    );
    entries.extend(surfaces.map(WorldLayer::Surface));
    for unit in units.iter().filter(|unit| unit.selected) {
        entries.extend(
            game_grid::selection_ring(camera, unit.position, unit.elevation_meters)
                .into_iter()
                .map(|(sprite, depth)| WorldLayer::Selection(sprite, depth)),
        );
    }
    entries.extend(
        objects
            .into_iter()
            .map(|(sprite, frame, depth, id)| WorldLayer::Sprite(sprite, frame, depth, id)),
    );
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

fn layer_order(layer: &WorldLayer) -> (f64, u8, u64) {
    match layer {
        WorldLayer::Surface(triangle) => (triangle_depth(triangle), 0, 0),
        WorldLayer::Selection(_, depth) => (*depth, 1, 0),
        WorldLayer::Sprite(_, _, depth, id) => (*depth, 2, *id),
    }
}

fn triangle_depth(triangle: &ProjectedSurfaceTriangle) -> f64 {
    triangle
        .points
        .iter()
        .map(|point| surface_render_depth(point.world, triangle.skirt))
        .sum::<f64>()
        / 3.0
}

enum WorldLayer {
    Surface(ProjectedSurfaceTriangle),
    Selection(Sprite, f64),
    Sprite(Sprite, GameFrame, f64, u64),
}

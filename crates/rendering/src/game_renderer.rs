//! WebGPU-first game rendering with WebGL2 acceleration and Canvas compatibility.
use crate::{
    GAME_ATLAS_SIDE, GameArt, GameFrame, Renderer, game_grid,
    playground::game_sprites,
    surface_mesh::{
        ProjectedSurfaceTriangle, apply_terrain_textures, projected_surface_triangles,
        surface_depth, surface_render_depth,
    },
    web::Sprite,
};
use aoe_core::Camera;
use wasm_bindgen::{Clamped, JsCast, JsValue};
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, ImageData};

#[path = "game_renderer/world_sprites.rs"]
mod world_sprites;
pub use world_sprites::scene_resource_frame;
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
#[path = "game_renderer/webgl.rs"]
mod webgl;
use webgl::WebGlRenderer;

#[path = "game_renderer/tests/filter_fixture.rs"]
#[cfg(test)]
pub(crate) mod filter_fixture;

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "game_renderer/tests/canvas_depth.rs"]
mod canvas_depth_tests;

#[cfg(test)]
#[path = "game_renderer/tests/webgl.rs"]
mod webgl_tests;

pub enum GameRenderer {
    WebGpu(Box<Renderer>),
    WebGl(WebGlRenderer),
    Canvas {
        canvas: HtmlCanvasElement,
        context: CanvasRenderingContext2d,
        atlas: [Option<HtmlCanvasElement>; 3],
        source_atlas: Vec<u8>,
        presentation: CanvasPresentation,
    },
}
#[path = "game_renderer/scene_types.rs"]
mod scene_types;
pub use scene_types::{
    SceneCamera, SceneDecoration, SceneResource, SceneTerrain, SceneTerrainAppearance,
    SceneTerrainSurface, SceneUnit,
};
fn error(e: impl Into<JsValue>) -> String {
    format!("Canvas rendering unavailable: {:?}", e.into())
}

fn replace_canvas(canvas: &HtmlCanvasElement) -> Result<HtmlCanvasElement, String> {
    let replacement: HtmlCanvasElement = canvas
        .clone_node()
        .map_err(error)?
        .dyn_into()
        .map_err(error)?;
    canvas
        .parent_node()
        .ok_or("Canvas is detached")?
        .replace_child(&replacement, canvas)
        .map_err(error)?;
    Ok(replacement)
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
        let replacement = replace_canvas(&canvas)?;
        if let Ok(renderer) = WebGlRenderer::new(&replacement) {
            return Ok((Self::WebGl(renderer), replacement));
        }
        // Either failed GPU tier may have bound the element's context type.
        let replacement = replace_canvas(&replacement)?;
        let main_context = context(&replacement)?;
        Ok((
            Self::Canvas {
                canvas: replacement.clone(),
                context: main_context,
                atlas: [None, None, None],
                source_atlas: Vec::new(),
                presentation: CanvasPresentation::new(replacement.width(), replacement.height()),
            },
            replacement,
        ))
    }

    pub fn backend(&self) -> &'static str {
        match self {
            Self::WebGpu(_) => "webgpu",
            Self::WebGl(_) => "webgl2",
            Self::Canvas { .. } => "canvas2d",
        }
    }

    pub fn upload_game_atlas(&mut self, pixels: &[u8]) -> Result<(), String> {
        match self {
            Self::WebGpu(renderer) => renderer.upload_game_atlas(pixels),
            Self::WebGl(renderer) => renderer.upload(pixels),
            Self::Canvas {
                atlas,
                source_atlas,
                ..
            } => {
                if pixels.len() != crate::GAME_ATLAS_BYTES {
                    return Err("Invalid game atlas size".into());
                }
                *atlas = [None, None, None];
                *source_atlas = pixels.to_vec();
                Ok(())
            }
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        match self {
            Self::WebGpu(renderer) => renderer.resize(width, height),
            Self::WebGl(renderer) => {
                let _ = renderer.resize(width, height);
            }
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
            Self::WebGl(renderer) => renderer
                .render(&mut game_sprites(
                    art, unit, target, moving, animation, facing,
                ))
                .map(|_| ()),
            Self::Canvas {
                canvas,
                context,
                atlas,
                source_atlas,
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
                    let page = legacy_page(canvas, atlas, source_atlas, sprite.pages[0])?;
                    let result = context.draw_image_with_html_canvas_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(page, sx, sy, sw.abs(), sh, 0.0, 0.0, w, h).map_err(error);
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
    ) -> Result<bool, String> {
        let surfaces = projected_surface_triangles(terrain, camera);
        self.render_prepared_world(
            art,
            terrain,
            &surfaces,
            resources,
            units,
            camera,
            animation,
            grid.then(|| game_grid::viewport_bounds(camera)),
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
        grid: Option<aoe_core::TileRect>,
    ) -> Result<bool, String> {
        let surfaces = surfaces.iter().copied().map(|mut triangle| {
            apply_terrain_textures(std::slice::from_mut(&mut triangle), art);
            triangle
        });
        let object_sprites = world_sprite_frames(art, terrain, resources, units, camera, animation);
        let layers = ordered_world_layers(surfaces, object_sprites, units, camera);
        if matches!(self, Self::WebGpu(_) | Self::WebGl(_)) {
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
            if let Some(bounds) = grid {
                instances.extend(game_grid::grid_sprites(camera, bounds));
            }
            return match self {
                Self::WebGpu(renderer) => renderer
                    .render_sprites_with_clear(&instances, [0.16, 0.29, 0.14, 1.0])
                    .map(|counters| counters.did_present),
                Self::WebGl(renderer) => renderer.render(&mut instances),
                _ => unreachable!(),
            };
        }
        match self {
            Self::Canvas {
                canvas,
                context,
                source_atlas,
                presentation,
                ..
            } => {
                if canvas.width() == 0 || canvas.height() == 0 {
                    return Ok(false);
                }
                render_canvas_world(
                    canvas,
                    context,
                    source_atlas,
                    presentation,
                    &layers,
                    camera,
                    false,
                )?;
                if let Some(bounds) = grid {
                    game_grid::draw_grid(context, camera, bounds);
                }
                Ok(true)
            }
            _ => unreachable!(),
        }
    }
}

fn legacy_page<'a>(
    canvas: &HtmlCanvasElement,
    pages: &'a mut [Option<HtmlCanvasElement>; 3],
    pixels: &[u8],
    page: u32,
) -> Result<&'a HtmlCanvasElement, String> {
    let slot = pages.get_mut(page as usize).ok_or("Invalid atlas page")?;
    if slot.is_none() {
        let start = page as usize * crate::GAME_ATLAS_PAGE_BYTES;
        let source = pixels
            .get(start..start + crate::GAME_ATLAS_PAGE_BYTES)
            .ok_or("Game atlas is not uploaded")?;
        let atlas = new_atlas(canvas)?;
        let data = ImageData::new_with_u8_clamped_array_and_sh(
            Clamped(source),
            GAME_ATLAS_SIDE,
            GAME_ATLAS_SIDE,
        )
        .map_err(error)?;
        context(&atlas)?
            .put_image_data(&data, 0.0, 0.0)
            .map_err(error)?;
        *slot = Some(atlas);
    }
    slot.as_ref()
        .ok_or_else(|| "Atlas canvas unavailable".into())
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
    // Original indices break all ties, preserving exact former stable order.
    // Sort a bounded integer sidecar and permute this scene in place, avoiding
    // a second large-layer scene or large-element stable-sort scratch buffer.
    crate::stable_index_sort::sort_by(&mut entries, |left, right| {
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

#[derive(Clone, Copy)]
enum WorldLayer {
    Surface(ProjectedSurfaceTriangle),
    Selection(Sprite, f64),
    Sprite(Sprite, GameFrame, f64, u64),
}

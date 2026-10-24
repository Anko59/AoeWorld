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

#[path = "game_renderer/fog.rs"]
pub(crate) mod fog;
#[path = "game_renderer/ground/mod.rs"]
mod ground;
pub use ground::apply_building_foundations;
#[path = "game_renderer/world_layers.rs"]
mod world_layers;
#[path = "game_renderer/world_sprites.rs"]
mod world_sprites;
use world_layers::direct_world_layers;
#[cfg(test)]
use world_layers::{layer_order, ordered_world_layers, triangle_depth};
#[cfg(test)]
use world_sprites::world_sprite_frames;

#[cfg(test)]
#[path = "game_renderer/tests/emission/direct_emission.rs"]
mod direct_emission_tests;
pub use world_sprites::{
    scene_building_visible, scene_resource_animation_frame, scene_resource_frame,
    scene_resource_presentation, scene_unit_bounds, scene_unit_depth, scene_unit_render_key,
};
pub fn scene_unit_animates(art: &GameArt, unit: SceneUnit, camera: SceneCamera) -> bool {
    world_sprites::scene_unit_animates(art, unit, camera)
}
pub use world_sprites::actor_frame_index;

pub fn resource_sprite_bounds(
    resource: SceneResource,
    frame: GameFrame,
    camera: SceneCamera,
) -> Option<[f64; 4]> {
    world_sprites::resource_sprite_bounds(resource, frame, camera)
}

#[path = "game_renderer/canvas_depth.rs"]
mod canvas_depth;
#[cfg(test)]
use canvas_depth::render_canvas_world;
use canvas_depth::{CanvasPresentation, render_canvas_world_with_fog};
#[path = "game_renderer/webgl.rs"]
mod webgl;
use webgl::WebGlRenderer;

#[path = "game_renderer/tests/filter_fixture.rs"]
#[cfg(test)]
pub(crate) mod filter_fixture;

#[path = "game_renderer/tests/species/fixture.rs"]
#[cfg(test)]
pub(crate) mod species_fixture;

#[path = "game_renderer/tests/tiling/wear.rs"]
#[cfg(test)]
pub(crate) mod wear_fixture;

#[path = "game_renderer/tests/tiling/relief.rs"]
#[cfg(test)]
pub(crate) mod relief_fixture;

#[cfg(test)]
#[path = "game_renderer/tests/canvas/alpha.rs"]
mod canvas_alpha_tests;
#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "game_renderer/tests/canvas_depth.rs"]
mod canvas_depth_tests;

#[cfg(test)]
#[path = "game_renderer/tests/minification/mod.rs"]
pub(crate) mod minification_tests;

#[cfg(test)]
#[path = "game_renderer/tests/webgl.rs"]
mod webgl_tests;

mod atlas_upload;
pub use atlas_upload::AtlasUpload;

pub enum GameRenderer {
    WebGpu(Box<Renderer>),
    WebGl(WebGlRenderer),
    Canvas {
        canvas: HtmlCanvasElement,
        context: CanvasRenderingContext2d,
        atlas: [Option<HtmlCanvasElement>; 4],
        source_atlas: Vec<u8>,
        presentation: CanvasPresentation,
    },
}
#[path = "game_renderer/scene_types.rs"]
mod scene_types;
pub use scene_types::{
    SceneCamera, SceneDecoration, SceneResource, SceneTerrain, SceneTerrainAppearance,
    SceneTerrainSurface, SceneUnit, SceneUnitKind, SceneUnitVisual,
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

/// Canvas already owns alpha compositing; keep float-to-CSS formatting out
/// of the WASM bundle and restore the caller's drawing state after each fill.
fn fill_solid(context: &CanvasRenderingContext2d, colour: [f32; 4], rect: [f64; 4]) {
    let [r, g, b, a] = colour;
    context.save();
    context.set_fill_style_str(&format!(
        "rgb({},{},{})",
        (r * 255.0) as u8,
        (g * 255.0) as u8,
        (b * 255.0) as u8
    ));
    context.set_global_alpha(context.global_alpha() * f64::from(a.clamp(0.0, 1.0)));
    context.fill_rect(rect[0], rect[1], rect[2], rect[3]);
    context.restore();
}

impl GameRenderer {
    pub async fn new(canvas: HtmlCanvasElement) -> Result<(Self, HtmlCanvasElement), String> {
        Self::new_with_backend(canvas, None).await
    }

    pub async fn new_with_backend(
        canvas: HtmlCanvasElement,
        backend: Option<&str>,
    ) -> Result<(Self, HtmlCanvasElement), String> {
        if backend != Some("canvas")
            && backend != Some("webgl2")
            && let Ok(renderer) = Renderer::new(canvas.clone()).await
        {
            return Ok((Self::WebGpu(Box::new(renderer)), canvas));
        }
        let replacement = replace_canvas(&canvas)?;
        if backend != Some("canvas")
            && let Ok(renderer) = WebGlRenderer::new(&replacement)
        {
            return Ok((Self::WebGl(renderer), replacement));
        }
        // Either failed GPU tier may have bound the element's context type.
        let replacement = replace_canvas(&replacement)?;
        let main_context = context(&replacement)?;
        Ok((
            Self::Canvas {
                canvas: replacement.clone(),
                context: main_context,
                atlas: [None, None, None, None],
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

    /// Takes the only WASM copy of the atlas. GPU tiers release it after
    /// upload; Canvas keeps it as its raster source without duplicating it.
    /// The art's terrain frame directory is written into the atlas first, so
    /// every tier resolves a world tile to its native frame from the same bytes.
    pub fn upload_game_atlas(&mut self, art: &GameArt, mut pixels: Vec<u8>) -> Result<(), String> {
        crate::write_terrain_directory(art, &mut pixels)?;
        match self {
            Self::WebGpu(renderer) => renderer.upload_game_atlas(&pixels),
            Self::WebGl(renderer) => renderer.upload(&pixels),
            Self::Canvas {
                atlas,
                source_atlas,
                ..
            } => {
                if pixels.len() != crate::GAME_ATLAS_BYTES {
                    return Err("Invalid game atlas size".into());
                }
                *atlas = [None, None, None, None];
                *source_atlas = pixels;
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
                        fill_solid(context, sprite.color, [x, y, w, h]);
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
    ) -> Result<bool, String> {
        self.render_prepared_world_with_fog(
            art,
            terrain,
            surfaces,
            resources,
            units,
            camera,
            animation,
            grid,
            false,
            &|_| 1.0,
        )
    }

    pub fn render_prepared_world_with_fog(
        &mut self,
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
        let surfaces = surfaces.iter().copied().map(|mut triangle| {
            apply_terrain_textures(std::slice::from_mut(&mut triangle), art);
            triangle
        });
        let mut layers =
            direct_world_layers(surfaces, art, terrain, resources, units, camera, animation);
        fog::shade_objects(&mut layers, art, resources, units, animation, light);
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
                        let mut packet =
                            crate::web::surface_instance(triangle, camera.viewport, depth_origin);
                        fog::surface_packet(&mut packet, fog::vertices(triangle, light));
                        packet
                    }
                    WorldLayer::Selection(sprite, depth) => {
                        let mut sprite = *sprite;
                        sprite.depths =
                            game_grid::selection_instance_depths(&sprite, *depth, depth_origin);
                        sprite
                    }
                    WorldLayer::Sprite(sprite, _, depth, _) => {
                        let mut sprite = *sprite;
                        sprite.depths = [(*depth - depth_origin) as f32; 4];
                        sprite
                    }
                })
                .collect::<Vec<_>>();
            if grid {
                instances.extend(game_grid::grid_sprites(
                    &fog::grid_segments(&layers, camera, light),
                    camera.viewport,
                ));
            }
            return match self {
                Self::WebGpu(renderer) => renderer
                    .render_sprites_with_clear(
                        &instances,
                        if fog_active {
                            [0.0, 0.0, 0.0, 1.0]
                        } else {
                            [0.16, 0.29, 0.14, 1.0]
                        },
                    )
                    .map(|counters| counters.did_present),
                Self::WebGl(renderer) => renderer.render_with_clear(&mut instances, fog_active),
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
                Ok(true)
            }
            _ => unreachable!(),
        }
    }
}

fn legacy_page<'a>(
    canvas: &HtmlCanvasElement,
    pages: &'a mut [Option<HtmlCanvasElement>; 4],
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

enum WorldLayer {
    Surface(ProjectedSurfaceTriangle),
    Selection(Sprite, f64),
    Sprite(Sprite, GameFrame, f64, u64),
}

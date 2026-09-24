//! WebGPU-first game rendering with a Canvas 2D compatibility path.
use crate::{
    GAME_ATLAS_SIDE, GameArt, GameFrame, Renderer, canvas_scene::draw_scene_sprite, game_grid,
    playground::game_sprites, surface_mesh::projected_surface_triangles,
    terrain::visible_terrain_frames, web::Sprite,
};
use aoe_core::{Camera, EntityId};
use wasm_bindgen::{Clamped, JsCast, JsValue};
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, ImageData};

pub enum GameRenderer {
    WebGpu(Box<Renderer>),
    Canvas {
        canvas: HtmlCanvasElement,
        context: CanvasRenderingContext2d,
        atlas: HtmlCanvasElement,
    },
}
#[derive(Clone, Copy)]
pub struct SceneCamera {
    pub center: [f64; 2],
    pub zoom: f64,
    pub viewport: [f64; 2],
    pub focus_elevation_meters: f64,
}

#[derive(Clone, Copy)]
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

#[derive(Clone, Copy)]
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
        let context = context(&replacement)?;
        let atlas: HtmlCanvasElement = replacement
            .owner_document()
            .ok_or("No document")?
            .create_element("canvas")
            .map_err(error)?
            .dyn_into()
            .map_err(error)?;
        atlas.set_width(GAME_ATLAS_SIDE);
        atlas.set_height(GAME_ATLAS_SIDE);
        Ok((
            Self::Canvas {
                canvas: replacement.clone(),
                context,
                atlas,
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
            Self::Canvas { atlas, .. } => {
                let data = ImageData::new_with_u8_clamped_array_and_sh(
                    Clamped(pixels),
                    GAME_ATLAS_SIDE,
                    GAME_ATLAS_SIDE,
                )
                .map_err(error)?;
                context(atlas)?
                    .put_image_data(&data, 0.0, 0.0)
                    .map_err(error)
            }
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if let Self::WebGpu(renderer) = self {
            renderer.resize(width, height);
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
        let sprites = world_sprites(art, terrain, resources, units, camera, animation, grid);
        let surfaces = projected_surface_triangles(terrain, camera);
        match self {
            Self::WebGpu(renderer) => renderer
                .render_world_layers(&surfaces, &sprites, [0.16, 0.29, 0.14, 1.0])
                .map(|_| ()),
            Self::Canvas {
                canvas,
                context,
                atlas,
            } => {
                let width = f64::from(canvas.width());
                let height = f64::from(canvas.height());
                context.set_fill_style_str("#294a26");
                context.fill_rect(0.0, 0.0, width, height);
                context.set_image_smoothing_enabled(false);
                crate::surface_mesh::draw_surface_mesh(context, &surfaces)?;
                if grid {
                    game_grid::draw_grid(context, camera);
                }
                for (sprite, frame, selected) in
                    world_sprite_frames(art, terrain, resources, units, camera, animation)
                {
                    draw_scene_sprite(context, atlas, canvas, (sprite, frame), selected)?;
                }
                Ok(())
            }
        }
    }
}

fn world_sprites(
    art: &GameArt,
    terrain: &[SceneTerrain],
    resources: &[SceneResource],
    units: &[SceneUnit],
    camera: SceneCamera,
    animation: usize,
    grid: bool,
) -> Vec<Sprite> {
    let mut sprites = if grid {
        game_grid::grid_sprites(camera)
    } else {
        Vec::new()
    };
    let frames = world_sprite_frames(art, terrain, resources, units, camera, animation);
    sprites.extend(frames.iter().map(|(sprite, _, _)| *sprite));
    for unit in units.iter().filter(|unit| unit.selected) {
        sprites.extend(game_grid::selection_ring(
            camera,
            unit.position,
            unit.elevation_meters,
        ));
    }
    sprites
}

fn world_sprite_frames(
    art: &GameArt,
    terrain: &[SceneTerrain],
    resources: &[SceneResource],
    units: &[SceneUnit],
    camera: SceneCamera,
    animation: usize,
) -> Vec<(Sprite, GameFrame, bool)> {
    let mut result = visible_terrain_frames(art, terrain, camera)
        .into_iter()
        .map(|(sprite, frame)| (sprite, frame, false))
        .collect::<Vec<_>>();
    let projection = Camera {
        center: camera.center,
        zoom: camera.zoom,
        viewport: camera.viewport,
        focus_elevation_meters: camera.focus_elevation_meters,
    };
    let mut objects = Vec::new();
    for resource in resources {
        let Some(frames) = art.resources.get(usize::from(resource.kind)) else {
            continue;
        };
        if frames.is_empty() {
            continue;
        }
        let Some(frame) = frames.get(usize::from(resource.visual_variant) % frames.len()) else {
            continue;
        };
        objects.push(WorldObject::Resource(*resource, *frame));
    }
    objects.extend(units.iter().copied().map(WorldObject::Unit));
    objects.sort_by(|a, b| {
        let a_screen = projection.world_to_screen_at_height(a.position(), a.elevation_meters());
        let b_screen = projection.world_to_screen_at_height(b.position(), b.elevation_meters());
        a_screen
            .y
            .total_cmp(&b_screen.y)
            .then_with(|| a_screen.x.total_cmp(&b_screen.x))
            .then_with(|| a.stable_id().cmp(&b.stable_id()))
    });
    for object in objects {
        let WorldObject::Unit(unit) = object else {
            let WorldObject::Resource(resource, frame) = object else {
                continue;
            };
            if let Some(sprite) =
                scene_sprite(frame, resource.position, resource.elevation_meters, camera)
            {
                result.push((sprite, scaled(frame, camera.zoom as f32), false));
            }
            continue;
        };
        let screen = projection.world_to_screen_at_height(unit.position, unit.elevation_meters);
        let frames = if unit.moving {
            &art.walking
        } else {
            &art.standing
        };
        if frames.is_empty() {
            continue;
        }
        let (direction, flipped) = sprite_direction(unit.facing);
        let frame = frames[direction * 10 + if unit.moving { animation % 10 } else { 0 }];
        let scale = camera.zoom as f32;
        let width = f64::from(frame.size[0]) * camera.zoom;
        let height = f64::from(frame.size[1]) * camera.zoom;
        if screen.x + width < 0.0
            || screen.y + height < 0.0
            || screen.x - width > camera.viewport[0]
            || screen.y - height > camera.viewport[1]
        {
            continue;
        }
        let mut uv = frame.uv;
        if flipped {
            uv[0] += uv[2];
            uv[2] = -uv[2];
        }
        let x = screen.x
            - if flipped {
                width - f64::from(frame.anchor[0]) * camera.zoom
            } else {
                f64::from(frame.anchor[0]) * camera.zoom
            };
        let y = screen.y - f64::from(frame.anchor[1]) * camera.zoom;
        let sprite = Sprite {
            position: [
                ((x + width / 2.0) / camera.viewport[0] * 2.0 - 1.0) as f32,
                (1.0 - (y + height / 2.0) / camera.viewport[1] * 2.0) as f32,
            ],
            radius: [
                (width / camera.viewport[0]) as f32,
                (height / camera.viewport[1]) as f32,
            ],
            color: [1.0; 4],
            uv,
        };
        let mut scaled_frame = frame;
        scaled_frame.size = scaled_frame.size.map(|value| value * scale);
        scaled_frame.anchor = scaled_frame.anchor.map(|value| value * scale);
        result.push((sprite, scaled_frame, unit.selected));
    }
    result
}

#[derive(Clone, Copy)]
enum WorldObject {
    Resource(SceneResource, GameFrame),
    Unit(SceneUnit),
}

impl WorldObject {
    fn position(self) -> [f64; 2] {
        match self {
            Self::Resource(resource, _) => resource.position,
            Self::Unit(unit) => unit.position,
        }
    }

    fn stable_id(self) -> u64 {
        match self {
            Self::Resource(resource, _) => resource.id,
            Self::Unit(unit) => u64::from(unit.id.0),
        }
    }

    fn elevation_meters(self) -> f64 {
        match self {
            Self::Resource(resource, _) => resource.elevation_meters,
            Self::Unit(unit) => unit.elevation_meters,
        }
    }
}

fn scene_sprite(
    frame: GameFrame,
    position: [f64; 2],
    elevation_meters: f64,
    camera: SceneCamera,
) -> Option<Sprite> {
    let projection = Camera {
        center: camera.center,
        zoom: camera.zoom,
        viewport: camera.viewport,
        focus_elevation_meters: camera.focus_elevation_meters,
    };
    let screen = projection.world_to_screen_at_height(position, elevation_meters);
    let width = f64::from(frame.size[0]) * camera.zoom;
    let height = f64::from(frame.size[1]) * camera.zoom;
    if screen.x + width < 0.0
        || screen.y + height < 0.0
        || screen.x - width > camera.viewport[0]
        || screen.y - height > camera.viewport[1]
    {
        return None;
    }
    let x = screen.x - f64::from(frame.anchor[0]) * camera.zoom;
    let y = screen.y - f64::from(frame.anchor[1]) * camera.zoom;
    Some(Sprite {
        position: [
            ((x + width / 2.0) / camera.viewport[0] * 2.0 - 1.0) as f32,
            (1.0 - (y + height / 2.0) / camera.viewport[1] * 2.0) as f32,
        ],
        radius: [
            (width / camera.viewport[0]) as f32,
            (height / camera.viewport[1]) as f32,
        ],
        color: [1.0; 4],
        uv: frame.uv,
    })
}

fn scaled(mut frame: GameFrame, scale: f32) -> GameFrame {
    frame.size = frame.size.map(|value| value * scale);
    frame.anchor = frame.anchor.map(|value| value * scale);
    frame
}

fn sprite_direction(facing: u8) -> (usize, bool) {
    let facing = facing % 8;
    let row = [0_usize, 1, 2, 3, 4, 3, 2, 1][usize::from(facing)];
    (row, matches!(facing, 1..=3))
}

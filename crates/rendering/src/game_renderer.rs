//! WebGPU-first game rendering with a Canvas 2D compatibility path.
use crate::{GAME_ATLAS_SIDE, GameArt, GameFrame, Renderer, playground::game_sprites, web::Sprite};
use aoe_core::{Camera, EntityId, ScreenPoint};
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
}

#[derive(Clone, Copy)]
pub struct SceneUnit {
    pub id: EntityId,
    pub position: [f64; 2],
    pub moving: bool,
    pub facing: u8,
    pub selected: bool,
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
        units: &[SceneUnit],
        camera: SceneCamera,
        animation: usize,
        grid: bool,
    ) -> Result<(), String> {
        let sprites = world_sprites(art, units, camera, animation, grid);
        match self {
            Self::WebGpu(renderer) => renderer
                .render_sprites_with_clear(&sprites, [0.16, 0.29, 0.14, 1.0])
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
                if grid {
                    draw_grid(context, camera);
                }
                for (sprite, frame, selected) in world_sprite_frames(art, units, camera, animation)
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
    units: &[SceneUnit],
    camera: SceneCamera,
    animation: usize,
    grid: bool,
) -> Vec<Sprite> {
    let mut sprites = if grid {
        grid_sprites(camera)
    } else {
        Vec::new()
    };
    let frames = world_sprite_frames(art, units, camera, animation);
    sprites.extend(frames.iter().map(|(sprite, _, _)| *sprite));
    for unit in units.iter().filter(|unit| unit.selected) {
        sprites.extend(selection_ring(camera, unit.position));
    }
    sprites
}

fn world_sprite_frames(
    art: &GameArt,
    units: &[SceneUnit],
    camera: SceneCamera,
    animation: usize,
) -> Vec<(Sprite, GameFrame, bool)> {
    let mut result = Vec::with_capacity(units.len());
    let projection = Camera {
        center: camera.center,
        zoom: camera.zoom,
        viewport: camera.viewport,
    };
    let mut ordered = units.to_vec();
    ordered.sort_by(|a, b| {
        let a_screen = projection.world_to_screen(a.position);
        let b_screen = projection.world_to_screen(b.position);
        a_screen
            .y
            .total_cmp(&b_screen.y)
            .then_with(|| a_screen.x.total_cmp(&b_screen.x))
            .then_with(|| a.id.cmp(&b.id))
    });
    for unit in &ordered {
        let screen = projection.world_to_screen(unit.position);
        let frames = if unit.moving {
            &art.walking
        } else {
            &art.standing
        };
        if frames.is_empty() {
            continue;
        }
        let direction = [0_usize, 1, 2, 3, 4, 3, 2, 1][usize::from(unit.facing) % 8];
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
        let flipped = unit.facing >= 5;
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

fn selection_ring(camera: SceneCamera, position: [f64; 2]) -> Vec<Sprite> {
    let screen = Camera {
        center: camera.center,
        zoom: camera.zoom,
        viewport: camera.viewport,
    }
    .world_to_screen(position);
    (0..32)
        .map(|index| {
            let angle = f64::from(index) * std::f64::consts::TAU / 32.0;
            let x = screen.x + angle.cos() * 24.0 * camera.zoom;
            let y = screen.y + angle.sin() * 8.0 * camera.zoom;
            Sprite {
                position: [
                    (x / camera.viewport[0] * 2.0 - 1.0) as f32,
                    (1.0 - y / camera.viewport[1] * 2.0) as f32,
                ],
                radius: [
                    1.5 / camera.viewport[0] as f32,
                    1.5 / camera.viewport[1] as f32,
                ],
                color: [0.95, 0.85, 0.35, 1.0],
                uv: [
                    1.0 / GAME_ATLAS_SIDE as f32,
                    0.0,
                    1.0 / GAME_ATLAS_SIDE as f32,
                    1.0 / GAME_ATLAS_SIDE as f32,
                ],
            }
        })
        .collect()
}

fn grid_sprites(camera: SceneCamera) -> Vec<Sprite> {
    let projection = Camera {
        center: camera.center,
        zoom: camera.zoom,
        viewport: camera.viewport,
    };
    let visible = projection.visible_tiles(aoe_core::WorldConfig::default(), 2.0);
    let mut sprites = Vec::new();
    let width = camera.viewport[0].max(1.0);
    let height = camera.viewport[1].max(1.0);
    for tile in visible.min.x..=visible.max.x {
        let a = projection.world_to_screen([f64::from(tile), f64::from(visible.min.y)]);
        let b = projection.world_to_screen([f64::from(tile), f64::from(visible.max.y)]);
        add_line(&mut sprites, a, b, width, height);
    }
    for tile in visible.min.y..=visible.max.y {
        let a = projection.world_to_screen([f64::from(visible.min.x), f64::from(tile)]);
        let b = projection.world_to_screen([f64::from(visible.max.x), f64::from(tile)]);
        add_line(&mut sprites, a, b, width, height);
    }
    sprites
}

fn add_line(sprites: &mut Vec<Sprite>, a: ScreenPoint, b: ScreenPoint, width: f64, height: f64) {
    let distance = ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt();
    let steps = (distance / 18.0).ceil() as usize;
    if steps == 0 {
        return;
    }
    for step in 0..steps {
        let start = f64::from(step as u32) / f64::from(steps as u32);
        let end = f64::from((step + 1) as u32) / f64::from(steps as u32);
        let x = a.x + (b.x - a.x) * (start + end) * 0.5;
        let y = a.y + (b.y - a.y) * (start + end) * 0.5;
        let w = (b.x - a.x).abs() / f64::from(steps as u32) + 1.0;
        let h = (b.y - a.y).abs() / f64::from(steps as u32) + 1.0;
        sprites.push(Sprite {
            position: [
                (x / width * 2.0 - 1.0) as f32,
                (1.0 - y / height * 2.0) as f32,
            ],
            radius: [w as f32 / width as f32, h as f32 / height as f32],
            color: [0.75, 0.9, 0.6, 0.16],
            uv: [
                1.0 / GAME_ATLAS_SIDE as f32,
                0.0,
                1.0 / GAME_ATLAS_SIDE as f32,
                1.0 / GAME_ATLAS_SIDE as f32,
            ],
        });
    }
}

fn draw_scene_sprite(
    context: &CanvasRenderingContext2d,
    atlas: &HtmlCanvasElement,
    canvas: &HtmlCanvasElement,
    sprite: (Sprite, GameFrame),
    selected: bool,
) -> Result<(), String> {
    let (sprite, frame) = sprite;
    let width = f64::from(canvas.width());
    let height = f64::from(canvas.height());
    let x = (f64::from(sprite.position[0]) + 1.0) * width / 2.0 - f64::from(frame.size[0]) / 2.0;
    let y = (1.0 - f64::from(sprite.position[1])) * height / 2.0 - f64::from(frame.anchor[1]);
    let [sx, sy, sw, sh] = frame
        .uv
        .map(|value| f64::from(value) * f64::from(GAME_ATLAS_SIDE));
    context
        .draw_image_with_html_canvas_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
            atlas,
            sx,
            sy,
            sw,
            sh,
            x,
            y,
            f64::from(frame.size[0]),
            f64::from(frame.size[1]),
        )
        .map_err(error)?;
    if selected {
        context.begin_path();
        context.set_stroke_style_str("#f2dc78");
        context
            .ellipse(
                x + f64::from(frame.size[0]) / 2.0,
                y + f64::from(frame.size[1]),
                22.0,
                8.0,
                0.0,
                0.0,
                std::f64::consts::TAU,
            )
            .map_err(error)?;
        context.stroke();
    }
    Ok(())
}

fn draw_grid(context: &CanvasRenderingContext2d, camera: SceneCamera) {
    let projection = Camera {
        center: camera.center,
        zoom: camera.zoom,
        viewport: camera.viewport,
    };
    let left = projection.screen_to_world(ScreenPoint { x: 0.0, y: 0.0 });
    let right = projection.screen_to_world(ScreenPoint {
        x: camera.viewport[0],
        y: camera.viewport[1],
    });
    context.begin_path();
    context.set_stroke_style_str("rgba(220,235,170,.18)");
    let min = left[0].min(left[1]).floor() as i32 - 2;
    let max = right[0].max(right[1]).ceil() as i32 + 2;
    for index in min..=max {
        let a = projection.world_to_screen([f64::from(index), 0.0]);
        let b = projection.world_to_screen([f64::from(index), f64::from(max)]);
        context.move_to(a.x, a.y);
        context.line_to(b.x, b.y);
        let a = projection.world_to_screen([0.0, f64::from(index)]);
        let b = projection.world_to_screen([f64::from(max), f64::from(index)]);
        context.move_to(a.x, a.y);
        context.line_to(b.x, b.y);
    }
    context.stroke();
}

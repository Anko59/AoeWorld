//! Local AoE II atlas rendering through the shared WebGPU sprite pipeline.
use crate::{Counters, Renderer, web::Sprite};

pub const GAME_ATLAS_SIDE: u32 = 2048;

#[derive(Clone, Copy)]
pub struct GameFrame {
    pub uv: [f32; 4],
    pub size: [f32; 2],
    pub anchor: [f32; 2],
}

pub struct GameArt {
    pub walking: Vec<GameFrame>,
    pub standing: Vec<GameFrame>,
    pub grass: Vec<GameFrame>,
    pub trees: Vec<GameFrame>,
}

impl Renderer {
    pub fn upload_game_atlas(&mut self, pixels: &[u8]) -> Result<(), String> {
        let side = GAME_ATLAS_SIDE;
        if pixels.len() != (side * side * 4) as usize {
            return Err("Invalid game atlas size".into());
        }
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("local AoE II game atlas"),
            size: wgpu::Extent3d {
                width: side,
                height: side,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            texture.as_image_copy(),
            pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(side * 4),
                rows_per_image: Some(side),
            },
            wgpu::Extent3d {
                width: side,
                height: side,
                depth_or_array_layers: 1,
            },
        );
        let view = texture.create_view(&Default::default());
        let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        self.bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("local game sprites"),
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        self._atlas = texture;
        Ok(())
    }

    pub fn render_game(
        &mut self,
        art: &GameArt,
        unit: [f32; 2],
        target: [f32; 2],
        moving: bool,
        animation: usize,
        facing: (usize, bool),
    ) -> Result<Counters, String> {
        let sprites = game_sprites(art, unit, target, moving, animation, facing);
        let mut counters = self.render_sprites(&sprites)?;
        counters.atlas_bytes = (GAME_ATLAS_SIDE * GAME_ATLAS_SIDE * 4) as usize;
        Ok(counters)
    }
}

pub(crate) fn game_sprites(
    art: &GameArt,
    unit: [f32; 2],
    target: [f32; 2],
    moving: bool,
    animation: usize,
    facing: (usize, bool),
) -> Vec<Sprite> {
    let mut sprites = Vec::new();
    for row in -1_i32..28 {
        for column in -1_i32..11 {
            let frame =
                art.grass[(row * 7 + column * 13).unsigned_abs() as usize % art.grass.len()];
            push(
                &mut sprites,
                frame,
                [
                    column as f32 * 96.0 + (row % 2) as f32 * 48.0,
                    row as f32 * 24.0,
                ],
                1.0,
                false,
            );
        }
    }
    // Trees sit outside the traversable clearing.
    for i in 0..12 {
        let frame = art.trees[i % art.trees.len()];
        push(
            &mut sprites,
            frame,
            [i as f32 * 95.0 - 30.0, 20.0],
            1.0,
            i % 2 == 0,
        );
    }
    for i in 0..8 {
        let frame = art.trees[i % art.trees.len()];
        push(
            &mut sprites,
            frame,
            [-15.0, i as f32 * 95.0 + 70.0],
            1.0,
            false,
        );
        push(
            &mut sprites,
            frame,
            [985.0, i as f32 * 95.0 + 70.0],
            1.0,
            true,
        );
    }
    if moving {
        ring(&mut sprites, target, [0.95, 0.79, 0.3, 1.0], 12.0);
    }
    ring(&mut sprites, unit, [0.85, 0.95, 0.65, 1.0], 22.0);
    let frames = if moving { &art.walking } else { &art.standing };
    let frame = frames[facing.0 * 10 + if moving { animation % 10 } else { 0 }];
    push(&mut sprites, frame, unit, 1.35, facing.1);
    sprites
}

fn push(
    sprites: &mut Vec<Sprite>,
    frame: GameFrame,
    position: [f32; 2],
    scale: f32,
    flipped: bool,
) {
    let [w, h] = frame.size.map(|n| n * scale);
    let [ax, ay] = frame.anchor.map(|n| n * scale);
    let x = position[0] - if flipped { w - ax } else { ax };
    let y = position[1] - ay;
    let mut uv = frame.uv;
    if flipped {
        uv[0] += uv[2];
        uv[2] = -uv[2];
    }
    sprites.push(Sprite {
        position: [(x + w / 2.0) / 480.0 - 1.0, 1.0 - (y + h / 2.0) / 320.0],
        radius: [w / 960.0, h / 640.0],
        color: [1.0; 4],
        uv,
    });
}

fn ring(sprites: &mut Vec<Sprite>, p: [f32; 2], color: [f32; 4], radius: f32) {
    for i in 0..48 {
        let angle = i as f32 * std::f32::consts::TAU / 48.0;
        let x = p[0] + angle.cos() * radius;
        let y = p[1] + angle.sin() * radius * 0.45;
        sprites.push(Sprite {
            position: [x / 480.0 - 1.0, 1.0 - y / 320.0],
            radius: [1.2 / 480.0, 1.2 / 320.0],
            color,
            uv: [
                0.0,
                0.0,
                1.0 / GAME_ATLAS_SIDE as f32,
                1.0 / GAME_ATLAS_SIDE as f32,
            ],
        });
    }
}

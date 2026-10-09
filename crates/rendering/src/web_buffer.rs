use super::Sprite;
use super::gpu_bridge::{GpuBridge, error};
use crate::surface_mesh::ProjectedSurfaceTriangle;

pub(crate) const INITIAL_CAPACITY: usize = super::CAPACITY
    + crate::surface_mesh::MAX_SURFACE_TRIANGLES
    + crate::game_grid::SELECTION_RING_SPRITES;
/// Hard per-WebGPU instance-buffer resource bound. Supported frame lists grow
/// to their exact requirement up to this bound; larger lists fail before data
/// allocation or queue submission instead of overflowing the buffer.
pub(crate) const MAX_BUFFER_BYTES: u64 = 64 * 1024 * 1024;
pub(crate) const MAX_CAPACITY: usize =
    (MAX_BUFFER_BYTES / std::mem::size_of::<Sprite>() as u64) as usize;

pub(crate) struct InstanceBuffer {
    capacity: usize,
}

impl InstanceBuffer {
    pub(crate) fn new() -> Self {
        Self {
            capacity: INITIAL_CAPACITY,
        }
    }

    pub(crate) fn ensure_capacity(
        &mut self,
        required: usize,
        device: &GpuBridge,
    ) -> Result<(), String> {
        validate_capacity(required)?;
        if required <= self.capacity {
            return Ok(());
        }
        device.ensure_capacity(required as u32).map_err(error)?;
        self.capacity = required;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) const fn capacity(&self) -> usize {
        self.capacity
    }

    pub(crate) fn bytes(&self) -> usize {
        self.capacity * std::mem::size_of::<Sprite>()
    }
}

pub(crate) fn required_capacity(
    surfaces: &[ProjectedSurfaceTriangle],
    sprites: &[Sprite],
) -> Result<usize, String> {
    required_instance_count(surfaces.len(), sprites.len())
}

pub(crate) fn required_instance_count(surfaces: usize, sprites: usize) -> Result<usize, String> {
    let count = surfaces
        .checked_add(sprites)
        .ok_or_else(|| "WebGPU instance count overflowed".to_owned())?;
    validate_capacity(count)?;
    Ok(count)
}

fn validate_capacity(capacity: usize) -> Result<(), String> {
    if capacity > MAX_CAPACITY {
        return Err(format!(
            "visible world layers exceed the {MAX_BUFFER_BYTES}-byte WebGPU instance bound"
        ));
    }
    Ok(())
}

fn screen_to_clip(x: f64, y: f64, width: f64, height: f64) -> [f32; 2] {
    [
        (x / width * 2.0 - 1.0) as f32,
        (1.0 - y / height * 2.0) as f32,
    ]
}

/// Reconstruct the original legacy packet without changing atlas image bytes.
pub(crate) fn retain_world_packet(sprite: &mut Sprite, admitted: Option<u32>) {
    const WORLD: u32 = 1 << 30;
    if sprite.pages[3] & WORLD == 0 || admitted == Some(sprite.pages[2]) {
        return;
    }
    let primary_page = sprite.terrain_blend[1][2] as u32 / 8;
    let bed_page = sprite.terrain_blend[1][3] as u32 / 8;
    sprite.pages = [
        primary_page,
        bed_page,
        primary_page,
        sprite.pages[3] & !WORLD,
    ];
    if primary_page == bed_page && sprite.uv == sprite.terrain_blend[0] {
        // World metadata forced Some even for a single native group. Undo that
        // as well, including the optional floor payload, to match old emission.
        sprite.color[3] = -1.0;
        sprite.terrain_blend = [[0.0; 4]; 2];
        sprite.pages[1] = 0;
        sprite.pages[2] = 0;
        sprite.pages[3] &= !crate::surface_mesh::landscape::INTERPOLATED_FLOOR;
        sprite.depths[3] = 0.0;
    } else {
        sprite.terrain_blend[1] = sprite.uv;
    }
}

pub(crate) fn surface_instance(
    triangle: &ProjectedSurfaceTriangle,
    viewport: [f64; 2],
    depth_origin: f64,
) -> Sprite {
    let mut points = [[0.0; 2]; 3];
    let mut depths = [0.0; 3];
    for index in 0..3 {
        let point = triangle.points[index];
        points[index] = screen_to_clip(
            point.screen.x,
            point.screen.y,
            viewport[0].max(1.0),
            viewport[1].max(1.0),
        );
        depths[index] = (crate::surface_mesh::surface_render_depth(point.world, triangle.skirt)
            - depth_origin) as f32;
    }
    let landscape = matches!(triangle.material, 0 | 1 | 2 | 6)
        && triangle.tint <= 3
        && !triangle.skirt
        && triangle.appearance & 1 != 0;
    let floors = if landscape && triangle.texture_blend.is_some() {
        triangle.floor_strengths
    } else {
        None
    };
    let second = points[1];
    let third = points[2];
    let mut sprite = match triangle.texture_uv {
        Some(uv) => Sprite {
            position: points[0],
            radius: second,
            color: [
                third[0],
                third[1],
                f32::from(triangle.tint) * 8.0 + f32::from(triangle.texture_mode),
                if triangle.texture_blend.is_some() {
                    -3.0
                } else {
                    -1.0
                },
            ],
            uv: uv.uv,
            depths: [
                depths[0],
                depths[1],
                depths[2],
                floors.map_or(0.0, crate::surface_mesh::landscape::pack_floors),
            ],
            terrain_blend: triangle
                .texture_blend
                .map(|addresses| addresses.map(|address| address.uv))
                .unwrap_or([[0.0; 4]; 2]),
            pages: [
                uv.page,
                triangle
                    .texture_blend
                    .map_or(0, |addresses| addresses[0].page),
                triangle
                    .texture_blend
                    .map_or(0, |addresses| addresses[1].page),
                if matches!(triangle.material, 0 | 1 | 2 | 6)
                    && triangle.tint <= 3
                    && !triangle.skirt
                {
                    (triangle.appearance & !crate::surface_mesh::landscape::INTERPOLATED_FLOOR)
                        | if floors.is_some() {
                            crate::surface_mesh::landscape::INTERPOLATED_FLOOR
                        } else {
                            0
                        }
                } else {
                    0
                },
            ],
        },
        None => Sprite {
            position: points[0],
            radius: second,
            color: [third[0], third[1], 0.0, -2.0],
            uv: [triangle.color[0], triangle.color[1], triangle.color[2], 1.0],
            depths: [depths[0], depths[1], depths[2], 0.0],
            terrain_blend: [[0.0; 4]; 2],
            pages: [0; 4],
        },
    };
    if let Some(world) = triangle
        .world_texture()
        .filter(|_| landscape && triangle.texture_uv.is_some())
    {
        let pages = sprite.pages;
        sprite.terrain_blend[1] = [
            world.footprint[0],
            world.footprint[1],
            f32::from(world.groups[0]) + (pages[0] * 8) as f32,
            f32::from(world.groups[1]) + (pages[1] * 8) as f32,
        ];
        sprite.pages = [
            triangle.texture_tile[0] as u32,
            triangle.texture_tile[1] as u32,
            world.checksum,
            pages[3] | (1 << 30),
        ];
    }
    sprite
}

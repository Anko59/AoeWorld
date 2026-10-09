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
    let second = points[1];
    let third = points[2];
    match triangle.texture_uv {
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
            depths: [depths[0], depths[1], depths[2], 0.0],
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
                0,
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
    }
}

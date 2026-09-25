use super::Sprite;
use crate::surface_mesh::ProjectedSurfaceTriangle;
use wgpu::{BindGroup, BindGroupLayout, Buffer, Device, Queue, Sampler, TextureView};

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
    buffer: Buffer,
    bind_group: BindGroup,
    _atlas_view: TextureView,
    _sampler: Sampler,
    capacity: usize,
}

impl InstanceBuffer {
    pub(crate) fn new(
        device: &Device,
        layout: &BindGroupLayout,
        atlas_view: &TextureView,
        sampler: &Sampler,
    ) -> Self {
        let buffer = create_buffer(device, INITIAL_CAPACITY);
        let bind_group = create_bind_group(device, layout, &buffer, atlas_view, sampler);
        Self {
            buffer,
            bind_group,
            _atlas_view: atlas_view.clone(),
            _sampler: sampler.clone(),
            capacity: INITIAL_CAPACITY,
        }
    }

    pub(crate) fn ensure_capacity(
        &mut self,
        required: usize,
        device: &Device,
        layout: &BindGroupLayout,
    ) -> Result<(), String> {
        validate_capacity(required)?;
        if required <= self.capacity {
            return Ok(());
        }
        let buffer = create_buffer(device, required);
        let bind_group =
            create_bind_group(device, layout, &buffer, &self._atlas_view, &self._sampler);
        self.buffer = buffer;
        self.bind_group = bind_group;
        self.capacity = required;
        Ok(())
    }

    pub(crate) fn write(&self, queue: &Queue, instances: &[Sprite]) {
        debug_assert!(instances.len() <= self.capacity);
        if !instances.is_empty() {
            queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(instances));
        }
    }

    pub(crate) fn rebind_resources(
        &mut self,
        device: &Device,
        layout: &BindGroupLayout,
        atlas_view: &TextureView,
        sampler: &Sampler,
    ) {
        self._atlas_view = atlas_view.clone();
        self._sampler = sampler.clone();
        self.bind_group = create_bind_group(
            device,
            layout,
            &self.buffer,
            &self._atlas_view,
            &self._sampler,
        );
    }

    pub(crate) fn set_on(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_bind_group(0, &self.bind_group, &[]);
    }

    #[cfg(test)]
    pub(crate) const fn capacity(&self) -> usize {
        self.capacity
    }

    pub(crate) fn bytes(&self) -> usize {
        usize::try_from(self.buffer.size()).unwrap_or(usize::MAX)
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

fn create_buffer(device: &Device, capacity: usize) -> Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("sprite instances"),
        size: (capacity * std::mem::size_of::<Sprite>()) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn create_bind_group(
    device: &Device,
    layout: &BindGroupLayout,
    buffer: &Buffer,
    atlas_view: &TextureView,
    sampler: &Sampler,
) -> BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("sprite data"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(atlas_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

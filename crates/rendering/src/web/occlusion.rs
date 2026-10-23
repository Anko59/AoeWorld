//! GPU-only coverage: one byte per backing pixel and one extra instanced draw.
use super::{Renderer, draw, instance_buffer::InstanceBuffer};

pub(super) struct Mask {
    pub(super) texture: wgpu::Texture,
    pub(super) view: wgpu::TextureView,
}
impl Mask {
    pub(super) fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("object silhouette coverage"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Uint,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        Self { texture, view }
    }
}

pub(super) struct Contour {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
}
impl Contour {
    pub(super) fn new(
        device: &wgpu::Device,
        instances: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
        mask: &wgpu::TextureView,
    ) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("occlusion mask layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Uint,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        Self {
            pipeline: draw::contour_pipeline(device, instances, &layout, format),
            bind_group: Self::group(device, &layout, mask),
        }
    }
    fn group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        mask: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("occlusion mask"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(mask),
            }],
        })
    }
    pub(super) fn rebind(&mut self, device: &wgpu::Device, mask: &wgpu::TextureView) {
        self.bind_group = Self::group(device, &self.pipeline.get_bind_group_layout(1), mask);
    }
    fn draw(&self, pass: &mut wgpu::RenderPass<'_>, instances: &InstanceBuffer, count: u32) {
        pass.set_pipeline(&self.pipeline);
        instances.set_on(pass);
        pass.set_bind_group(1, &self.bind_group, &[]);
        pass.draw(0..6, 0..count);
    }
}

impl Renderer {
    /// Shared by presentation and GPU readback tests, including both passes.
    pub(crate) fn encode_world(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        count: u32,
        clear: wgpu::Color,
    ) {
        let depth = self.depth.create_view(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("world colour and occlusion coverage"),
                color_attachments: &[
                    Some(attachment(view, wgpu::LoadOp::Clear(clear))),
                    Some(attachment(
                        &self.mask.view,
                        wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    )),
                ],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            self.draw_on(&mut pass, count);
        }
        if self.has_occlusion
            && let Some(contour) = &self.occlusion
        {
            // The mask is sampled only after its world attachment pass ends.
            // World depth and final painter coverage preserve transparent holes
            // and equal-depth ties, without CPU overlap tests or extra packets.
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("hidden units and resource hover contours"),
                color_attachments: &[Some(attachment(view, wgpu::LoadOp::Load))],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            contour.draw(&mut pass, &self.instances, count);
        }
    }
}

fn attachment(
    view: &wgpu::TextureView,
    load: wgpu::LoadOp<wgpu::Color>,
) -> wgpu::RenderPassColorAttachment<'_> {
    wgpu::RenderPassColorAttachment {
        view,
        depth_slice: None,
        resolve_target: None,
        ops: wgpu::Operations {
            load,
            store: wgpu::StoreOp::Store,
        },
    }
}

//! Submit caller-owned packets without cloning a second GPU scene.
use super::*;

impl Renderer {
    pub(crate) fn render_owned_sprites(
        &mut self,
        sprites: &mut [Sprite],
        clear: [f64; 4],
    ) -> Result<Counters, String> {
        let visible_count = sprites.len();
        self.reserve_packets(required_capacity(&[], sprites)?)?;
        self.submit_packets(sprites, visible_count, clear)
    }

    pub(super) fn reserve_packets(&mut self, required: usize) -> Result<(), String> {
        self.instances.ensure_capacity(
            required,
            &self.device,
            &self.pipeline.get_bind_group_layout(0),
        )
    }

    pub(super) fn submit_packets(
        &mut self,
        instances: &mut [Sprite],
        visible_count: usize,
        clear: [f64; 4],
    ) -> Result<Counters, String> {
        for sprite in instances.iter_mut() {
            retain_world_packet(sprite, self.world_atlas);
        }
        normalize_depths(instances);
        self.instances.write(&self.queue, instances);
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(Counters {
                    did_present: false,
                    visible: visible_count,
                    draw_calls: 0,
                    gpu_buffer_bytes: self.instances.bytes(),
                    persistent_gpu_resources: 7,
                    atlas_pages: self._atlas.size().depth_or_array_layers as usize,
                    atlas_uploads: 1,
                    atlas_bytes: self._atlas.width() as usize
                        * self._atlas.height() as usize
                        * self._atlas.size().depth_or_array_layers as usize
                        * 4,
                });
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.config);
                return Ok(Counters {
                    did_present: false,
                    visible: visible_count,
                    draw_calls: 0,
                    gpu_buffer_bytes: self.instances.bytes(),
                    persistent_gpu_resources: 7,
                    atlas_pages: self._atlas.size().depth_or_array_layers as usize,
                    atlas_uploads: 1,
                    atlas_bytes: self._atlas.width() as usize
                        * self._atlas.height() as usize
                        * self._atlas.size().depth_or_array_layers as usize
                        * 4,
                });
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                return Err("WebGPU surface lost; reload to restore it".to_owned());
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err("WebGPU surface validation failed".to_owned());
            }
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let depth_view = self
            .depth
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("sprites"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("world layers"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: clear[0],
                            g: clear[1],
                            b: clear[2],
                            a: clear[3],
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            self.instances.set_on(&mut pass);
            pass.draw(0..6, 0..instances.len() as u32);
        }
        self.queue.submit(Some(encoder.finish()));
        self.queue.present(frame);
        Ok(Counters {
            did_present: true,
            visible: visible_count,
            draw_calls: usize::from(!instances.is_empty()),
            gpu_buffer_bytes: self.instances.bytes(),
            persistent_gpu_resources: 7,
            atlas_pages: self._atlas.size().depth_or_array_layers as usize,
            atlas_uploads: 1,
            atlas_bytes: self._atlas.width() as usize
                * self._atlas.height() as usize
                * self._atlas.size().depth_or_array_layers as usize
                * 4,
        })
    }
}

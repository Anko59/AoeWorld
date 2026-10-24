use super::*;

impl Renderer {
    pub(crate) fn render_world_layers(
        &mut self,
        surfaces: &[ProjectedSurfaceTriangle],
        sprites: &[Sprite],
        clear: [f64; 4],
    ) -> Result<Counters, String> {
        let required = required_capacity(surfaces, sprites)?;
        self.instances.ensure_capacity(
            required,
            &self.device,
            &self.pipeline.get_bind_group_layout(0),
        )?;
        let mut instances = Vec::with_capacity(required);
        let width = self.config.width.max(1) as f64;
        let height = self.config.height.max(1) as f64;
        for triangle in surfaces {
            instances.push(surface_instance(triangle, [width, height], 0.0));
        }
        instances.extend_from_slice(sprites);
        self.has_occlusion = normalize_depths(&mut instances);
        if self.has_occlusion && self.occlusion.is_none() {
            self.occlusion = Some(occlusion::Contour::new(
                &self.device,
                &self.pipeline.get_bind_group_layout(0),
                self.config.format,
                &self.mask.view,
            ));
        }
        self.draws = draw_ranges(&instances);
        if self.mixed_pipeline.is_none() && self.draws.iter().any(|(_, mixed)| *mixed) {
            self.mixed_pipeline = Some(draw::pipeline(
                &self.device,
                &self.pipeline.get_bind_group_layout(0),
                self.config.format,
                true,
            ));
        }
        self.instances.write(&self.queue, &instances);
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(Counters {
                    did_present: false,
                    visible: sprites.len(),
                    draw_calls: 0,
                    gpu_buffer_bytes: self.instances.bytes(),
                    persistent_gpu_resources: 9
                        + usize::from(self.mixed_pipeline.is_some())
                        + 2 * usize::from(self.occlusion.is_some()),
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
                    visible: sprites.len(),
                    draw_calls: 0,
                    gpu_buffer_bytes: self.instances.bytes(),
                    persistent_gpu_resources: 9
                        + usize::from(self.mixed_pipeline.is_some())
                        + 2 * usize::from(self.occlusion.is_some()),
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
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("sprites"),
            });
        self.encode_world(
            &mut encoder,
            &view,
            instances.len() as u32,
            wgpu::Color {
                r: clear[0],
                g: clear[1],
                b: clear[2],
                a: clear[3],
            },
        );
        self.queue.submit(Some(encoder.finish()));
        self.queue.present(frame);
        Ok(Counters {
            did_present: true,
            visible: sprites.len(),
            draw_calls: self.draws.len() + usize::from(self.has_occlusion),
            gpu_buffer_bytes: self.instances.bytes(),
            persistent_gpu_resources: 9
                + usize::from(self.mixed_pipeline.is_some())
                + 2 * usize::from(self.occlusion.is_some()),
            atlas_pages: self._atlas.size().depth_or_array_layers as usize,
            atlas_uploads: 1,
            atlas_bytes: self._atlas.width() as usize
                * self._atlas.height() as usize
                * self._atlas.size().depth_or_array_layers as usize
                * 4,
        })
    }
}

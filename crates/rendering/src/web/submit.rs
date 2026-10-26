use super::*;

impl Renderer {
    pub(crate) fn render_world_layers(
        &mut self,
        surfaces: &[ProjectedSurfaceTriangle],
        sprites: &[Sprite],
        clear: [f64; 4],
    ) -> Result<Counters, String> {
        let required = required_capacity(surfaces, sprites)?;
        if !self.health.ready()? {
            return Ok(self.counters(sprites.len(), false));
        }
        // Keep one bounded CPU packet allocation through camera/animation churn.
        let mut instances = std::mem::take(&mut self.staging);
        instances.clear();
        instances.reserve_exact(required);
        let width = self.config.width.max(1) as f64;
        let height = self.config.height.max(1) as f64;
        for triangle in surfaces {
            instances.push(surface_instance(triangle, [width, height], 0.0));
        }
        instances.extend_from_slice(sprites);
        let result = self.render_instances(&mut instances, clear);
        self.staging = instances;
        result.map(|mut counters| {
            counters.visible = sprites.len();
            counters
        })
    }

    pub(crate) fn staging_bytes(&self) -> usize {
        self.staging.capacity() * std::mem::size_of::<Sprite>()
            + self.draws.capacity() * std::mem::size_of::<(std::ops::Range<u32>, bool)>()
            + self.contour_draws.capacity() * std::mem::size_of::<std::ops::Range<u32>>()
    }

    /// Fresh world depths, normalized in caller-owned scratch before one upload.
    pub(crate) fn render_instances(
        &mut self,
        instances: &mut [Sprite],
        clear: [f64; 4],
    ) -> Result<Counters, String> {
        let required = instance_buffer::required_instance_count(0, instances.len())?;
        if !self.health.ready()? {
            return Ok(self.counters(instances.len(), false));
        }
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(self.counters(instances.len(), false));
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.config);
                return Ok(self.counters(instances.len(), false));
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                return Err("WebGPU surface lost; reload to restore it".to_owned());
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err("WebGPU surface validation failed".to_owned());
            }
        };
        self.instances.ensure_capacity(required, &self.device)?;
        self.has_occlusion = normalize_depths(instances);
        self.contour_draws.clear();
        if self.has_occlusion {
            draw::write_contour_ranges(instances, &mut self.contour_draws);
        }
        if self.has_occlusion && self.occlusion.is_none() {
            self.occlusion = Some(occlusion::Contour::new(
                &self.device,
                &self.pipeline.get_bind_group_layout(0),
                self.config.format,
                &self.mask.view,
            ));
        }
        draw::write_draw_ranges(instances, &mut self.draws);
        if self.mixed_pipeline.is_none() && self.draws.iter().any(|(_, mixed)| *mixed) {
            self.mixed_pipeline = Some(draw::pipeline(
                &self.device,
                &self.pipeline.get_bind_group_layout(0),
                self.config.format,
                true,
            ));
        }
        self.instances.write(&self.queue, instances);
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
        self.health.submitted(&self.queue, false);
        Ok(self.counters(instances.len(), true))
    }

    fn counters(&self, visible: usize, did_present: bool) -> Counters {
        Counters {
            did_present,
            visible,
            draw_calls: if did_present {
                self.draws.len() + self.contour_draws.len()
            } else {
                0
            },
            gpu_buffer_bytes: self.instances.bytes(),
            persistent_gpu_resources: 10
                + usize::from(self.mixed_pipeline.is_some())
                + 2 * usize::from(self.occlusion.is_some()),
            atlas_pages: self._atlas.size().depth_or_array_layers as usize,
            atlas_uploads: 1,
            atlas_bytes: self._atlas.width() as usize
                * self._atlas.height() as usize
                * self._atlas.size().depth_or_array_layers as usize
                * 4,
        }
    }
}

//! Submit caller-owned packets through the same graphics driver as pixel tests.
use super::*;

impl Renderer {
    pub(super) fn reserve_packets(&mut self, required: usize) -> Result<(), String> {
        self.instances.ensure_capacity(required, &self.device)
    }

    pub(super) fn submit_packets(
        &mut self,
        instances: &mut [Sprite],
        visible_count: usize,
        clear: [f64; 4],
    ) -> Result<Counters, String> {
        normalize_depths(instances);
        if let Some(message) = &self.resize_error {
            return Err(message.clone());
        }
        // Synchronous borrowed memory view: the driver never retains this packet.
        // The pinned browser backend only produces Good/Lost acquisition outcomes.
        let did_present = self
            .device
            .render(bytemuck::cast_slice(instances), &clear)
            .map_err(error)?;
        Ok(Counters {
            did_present,
            visible: visible_count,
            draw_calls: usize::from(did_present && !instances.is_empty()),
            gpu_buffer_bytes: self.instances.bytes(),
            persistent_gpu_resources: 7,
            atlas_pages: self.atlas_pages as usize,
            atlas_uploads: 1,
            atlas_bytes: self.atlas_side as usize
                * self.atlas_side as usize
                * self.atlas_pages as usize
                * 4,
        })
    }
}

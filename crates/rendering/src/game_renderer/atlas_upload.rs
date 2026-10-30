//! Streamed atlas upload: one page at a time, never a whole CPU atlas for WebGPU.
use super::GameRenderer;

pub(super) enum Staging {
    Gpu(wgpu::Texture, Vec<u8>),
    Cpu(Vec<u8>),
}

/// An atlas being streamed page by page; see `GameRenderer::begin_game_atlas`.
mod source;
pub use source::AtlasSourceCopy;

pub struct AtlasUpload(Staging, u32, Option<wgpu::Texture>);

impl AtlasUpload {
    /// CPU backends already retain every page. Expose that same allocation so
    /// the loader can decode each source once and fill all its destinations.
    /// WebGPU keeps its one-page scratch buffer and never takes this path.
    pub fn resident_buffer(&mut self) -> Option<&mut [u8]> {
        match &mut self.0 {
            Staging::Cpu(pixels) => {
                pixels.resize(self.1 as usize * crate::GAME_ATLAS_PAGE_BYTES, 0);
                Some(pixels)
            }
            Staging::Gpu(_, _) => None,
        }
    }

    /// The zeroed buffer for page `layer`, to be filled in place and then
    /// committed with `GameRenderer::write_game_atlas_page`. GPU uploads reuse
    /// one page; CPU tiers write straight into the atlas they must retain.
    pub fn page_buffer(&mut self, layer: u32) -> Result<&mut [u8], String> {
        let page = crate::GAME_ATLAS_PAGE_BYTES;
        if layer >= self.1 {
            return Err("Invalid game atlas page".into());
        }
        match &mut self.0 {
            Staging::Gpu(_, scratch) => {
                scratch.clear();
                scratch.resize(page, 0);
                Ok(scratch)
            }
            Staging::Cpu(pixels) => {
                if pixels.len() != layer as usize * page {
                    return Err("Game atlas pages must arrive in order".into());
                }
                pixels.resize((layer as usize + 1) * page, 0);
                Ok(&mut pixels[layer as usize * page..])
            }
        }
    }
}

impl GameRenderer {
    /// Reused frame packets and draw ranges, separate from atlas/GPU payload.
    pub fn frame_staging_bytes(&self) -> usize {
        match self {
            Self::WebGpu(renderer) => renderer.staging_bytes(),
            _ => 0,
        }
    }

    /// Owned payload only; excludes driver overhead and browser swapchains.
    pub fn memory_usage(&self) -> [(&'static str, usize); 5] {
        let (textures, buffers, wasm_atlas, js_atlas, raster) = match self {
            Self::WebGpu(r) => {
                let (textures, buffers) = r.memory_usage();
                (textures, buffers, 0, 0, 0)
            }
            Self::WebGl(r) => (r.texture_bytes(), r.buffer_bytes(), r.source_bytes(), 0, 0),
            Self::Canvas {
                source_atlas,
                atlas,
                presentation,
                ..
            } => (
                0,
                0,
                source_atlas.capacity(),
                atlas
                    .iter()
                    .flatten()
                    .map(|c| c.width() as usize * c.height() as usize * 4)
                    .sum(),
                presentation.resident_bytes(),
            ),
        };
        [
            ("gpu_texture_bytes", textures),
            ("gpu_buffer_bytes", buffers),
            ("wasm_atlas_bytes", wasm_atlas),
            ("js_atlas_bytes", js_atlas),
            ("canvas_raster_bytes", raster),
        ]
    }

    /// Starts a streamed atlas upload. The previous atlas keeps rendering until
    /// `finish_game_atlas`. WebGPU receives each page straight into a fresh
    /// texture, so no CPU copy of the atlas exists; WebGL2 and Canvas gather
    /// the pages they must retain anyway (JS restoration copy, raster source).
    pub fn begin_game_atlas(&mut self, pages: u32) -> Result<AtlasUpload, String> {
        if !(crate::GAME_ATLAS_PAGES..=crate::MAX_GAME_ATLAS_PAGES).contains(&pages)
            || !pages.is_multiple_of(2)
        {
            return Err("Invalid atlas layer budget".into());
        }
        Ok(AtlasUpload(
            match self {
                Self::WebGpu(renderer) => {
                    renderer.check_health()?;
                    Staging::Gpu(renderer.create_game_atlas(pages)?, Vec::new())
                }
                Self::WebGl(_) | Self::Canvas { .. } => Staging::Cpu(Vec::with_capacity(
                    pages as usize * crate::GAME_ATLAS_PAGE_BYTES,
                )),
            },
            pages,
            None,
        ))
    }

    /// Commits the page filled through `AtlasUpload::page_buffer`.
    pub fn write_game_atlas_page(
        &mut self,
        upload: &mut AtlasUpload,
        layer: u32,
    ) -> Result<(), String> {
        if layer >= upload.1 {
            return Err("Invalid game atlas page".into());
        }
        match (&mut upload.0, &*self) {
            (Staging::Gpu(texture, scratch), Self::WebGpu(renderer))
                if scratch.len() == crate::GAME_ATLAS_PAGE_BYTES =>
            {
                renderer.health.allow_upload()?;
                renderer.write_game_atlas_page(texture, layer, scratch);
                renderer.health.submitted(&renderer.queue, true);
                Ok(())
            }
            (Staging::Cpu(pixels), Self::WebGl(_) | Self::Canvas { .. })
                if pixels.len() == (layer as usize + 1) * crate::GAME_ATLAS_PAGE_BYTES =>
            {
                Ok(())
            }
            _ => Err("Game atlas page was not filled in order".into()),
        }
    }

    /// The terrain directory belongs in page 0 before it is written.
    pub fn finish_game_atlas(&mut self, upload: AtlasUpload) -> Result<(), String> {
        match (upload.0, self) {
            (Staging::Gpu(texture, _), Self::WebGpu(renderer)) => {
                renderer.check_health()?;
                renderer.install_game_atlas(texture);
                Ok(())
            }
            (Staging::Cpu(pixels), Self::WebGl(renderer)) => renderer.upload_owned(pixels),
            (
                Staging::Cpu(pixels),
                Self::Canvas {
                    atlas,
                    source_atlas,
                    presentation,
                    ..
                },
            ) => {
                if pixels.len() != upload.1 as usize * crate::GAME_ATLAS_PAGE_BYTES {
                    return Err("Invalid game atlas size".into());
                }
                *atlas = [None, None, None, None];
                *source_atlas = pixels;
                presentation.invalidate();
                Ok(())
            }
            _ => Err("Game atlas upload does not match its renderer".into()),
        }
    }
}

#[cfg(test)]
mod tests;

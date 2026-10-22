//! Streamed atlas upload: one page at a time, never a whole CPU atlas for WebGPU.
use super::GameRenderer;

pub(super) enum Staging {
    Gpu(wgpu::Texture, Vec<u8>),
    Cpu(Vec<u8>),
}

/// An atlas being streamed page by page; see `GameRenderer::begin_game_atlas`.
pub struct AtlasUpload(Staging);

impl AtlasUpload {
    /// The zeroed buffer for page `layer`, to be filled in place and then
    /// committed with `GameRenderer::write_game_atlas_page`. GPU uploads reuse
    /// one page; CPU tiers write straight into the atlas they must retain.
    pub fn page_buffer(&mut self, layer: u32) -> Result<&mut [u8], String> {
        let page = crate::GAME_ATLAS_PAGE_BYTES;
        if layer >= crate::GAME_ATLAS_PAGES {
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
    /// Starts a streamed atlas upload. The previous atlas keeps rendering until
    /// `finish_game_atlas`. WebGPU receives each page straight into a fresh
    /// texture, so no CPU copy of the atlas exists; WebGL2 and Canvas gather
    /// the pages they must retain anyway (JS restoration copy, raster source).
    pub fn begin_game_atlas(&mut self) -> Result<AtlasUpload, String> {
        Ok(AtlasUpload(match self {
            Self::WebGpu(renderer) => Staging::Gpu(renderer.create_game_atlas()?, Vec::new()),
            Self::WebGl(_) | Self::Canvas { .. } => {
                Staging::Cpu(Vec::with_capacity(crate::GAME_ATLAS_BYTES))
            }
        }))
    }

    /// Commits the page filled through `AtlasUpload::page_buffer`.
    pub fn write_game_atlas_page(
        &mut self,
        upload: &mut AtlasUpload,
        layer: u32,
    ) -> Result<(), String> {
        if layer >= crate::GAME_ATLAS_PAGES {
            return Err("Invalid game atlas page".into());
        }
        match (&mut upload.0, &*self) {
            (Staging::Gpu(texture, scratch), Self::WebGpu(renderer))
                if scratch.len() == crate::GAME_ATLAS_PAGE_BYTES =>
            {
                renderer.write_game_atlas_page(texture, layer, scratch);
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
                renderer.install_game_atlas(texture);
                Ok(())
            }
            (Staging::Cpu(pixels), Self::WebGl(renderer)) => renderer.upload(&pixels),
            (
                Staging::Cpu(pixels),
                Self::Canvas {
                    atlas,
                    source_atlas,
                    ..
                },
            ) => {
                if pixels.len() != crate::GAME_ATLAS_BYTES {
                    return Err("Invalid game atlas size".into());
                }
                *atlas = [None, None, None, None];
                *source_atlas = pixels;
                Ok(())
            }
            _ => Err("Game atlas upload does not match its renderer".into()),
        }
    }
}

#[cfg(test)]
mod tests;

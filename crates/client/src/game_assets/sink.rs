//! Renderer-owned upload storage; CPU tiers reuse their retained full atlas.
use super::*;

pub struct RendererSink<'a> {
    renderer: &'a mut aoe_rendering::GameRenderer,
    upload: Option<aoe_rendering::AtlasUpload>,
}

impl<'a> RendererSink<'a> {
    pub fn new(renderer: &'a mut aoe_rendering::GameRenderer) -> Self {
        Self {
            renderer,
            upload: None,
        }
    }

    fn upload(&mut self) -> Result<&mut aoe_rendering::AtlasUpload, String> {
        if self.upload.is_none() {
            self.upload = Some(self.renderer.begin_game_atlas()?);
        }
        self.upload.as_mut().ok_or("No atlas upload".into())
    }
}

impl AtlasSink for RendererSink<'_> {
    fn resident_buffer(&mut self) -> Result<Option<&mut [u8]>, String> {
        Ok(self.upload()?.resident_buffer())
    }
    fn page_buffer(&mut self, index: u32) -> Result<&mut [u8], String> {
        self.upload()?.page_buffer(index)
    }
    fn commit(&mut self, index: u32) -> Result<(), String> {
        let upload = self.upload.as_mut().ok_or("No atlas upload")?;
        self.renderer.write_game_atlas_page(upload, index)
    }
    fn finish(&mut self) -> Result<(), String> {
        let upload = self.upload.take().ok_or("No atlas pages written")?;
        self.renderer.finish_game_atlas(upload)
    }
}

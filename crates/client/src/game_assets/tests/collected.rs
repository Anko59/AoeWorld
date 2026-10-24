//! Whole-atlas test sink, also usable through the page-only reference wrapper.
use super::{AtlasSink, GAME_ATLAS_BYTES, GAME_ATLAS_PAGE_BYTES};

pub(crate) struct CollectedAtlas(pub Vec<u8>);

impl AtlasSink for CollectedAtlas {
    fn resident_buffer(&mut self) -> Result<Option<&mut [u8]>, String> {
        self.0.resize(GAME_ATLAS_BYTES, 0);
        Ok(Some(&mut self.0))
    }
    fn page_buffer(&mut self, index: u32) -> Result<&mut [u8], String> {
        let end = (index as usize + 1) * GAME_ATLAS_PAGE_BYTES;
        self.0.resize(end, 0);
        Ok(&mut self.0[end - GAME_ATLAS_PAGE_BYTES..])
    }
    fn commit(&mut self, _: u32) -> Result<(), String> {
        Ok(())
    }
    fn finish(&mut self) -> Result<(), String> {
        Ok(())
    }
}

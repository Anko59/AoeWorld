//! Stream colour plus aligned owner masks, decoding one source plane at a time.
use super::*;

pub(super) struct Input<'a> {
    pub manifest: &'a Manifest,
    pub selected: &'a [usize],
    pub placements: &'a [Placement],
    pub maskable: &'a [bool],
    pub actor_ranges: &'a [Option<std::ops::Range<usize>>],
    pub art: &'a GameArt,
    pub team: [u8; 3],
}
impl Input<'_> {
    pub async fn upload(self, sink: &mut dyn AtlasSink) -> Result<(), JsValue> {
        if let Some(pixels) = sink.resident_buffer().map_err(error)? {
            self.upload_resident(pixels).await?;
            return sink.finish().map_err(error);
        }
        let source_order = source_order(self.manifest, self.selected);
        for page_index in 0..GAME_ATLAS_PAGES {
            let mask = page_index >= 5;
            let colour_page = if mask { page_index - 3 } else { page_index };
            let page_pixels = sink.page_buffer(page_index).map_err(error)?;
            if !mask {
                page_pixels[..4].copy_from_slice(&[255; 4]);
            }
            let mut start = 0;
            while start < source_order.len() {
                let source_page = self.manifest.frames[self.selected[source_order[start]]].page;
                let atlas = &self.manifest.pages[usize::from(source_page)];
                let mut end = start;
                while end < source_order.len()
                    && self.manifest.frames[self.selected[source_order[end]]].page == source_page
                {
                    end += 1;
                }
                let group = &source_order[start..end];
                start = end;
                if !group.iter().any(|&semantic| {
                    u32::from(self.placements[semantic].page) == colour_page
                        && (!mask || self.maskable[semantic])
                }) {
                    continue;
                }
                for (plane, name) in [
                    (placement::Plane::Color, &atlas.color),
                    (placement::Plane::Player, &atlas.player),
                    (placement::Plane::Shadow, &atlas.shadow),
                ] {
                    if mask && matches!(plane, placement::Plane::Shadow) {
                        continue;
                    }
                    let source = page(name).await?;
                    for &semantic in group {
                        let mut placed = self.placements[semantic];
                        if u32::from(placed.page) != colour_page
                            || (mask && !self.maskable[semantic])
                        {
                            continue;
                        }
                        let owned = self.maskable[semantic];
                        let plane = if mask {
                            placed.page += 3;
                            if matches!(plane, placement::Plane::Color) {
                                placement::Plane::MaskBase
                            } else {
                                placement::Plane::MaskPlayer
                            }
                        } else if owned && matches!(plane, placement::Plane::Player) {
                            placement::Plane::PlayerCoverage
                        } else {
                            plane
                        };
                        placement::copy_plane(
                            &self.manifest.frames[self.selected[semantic]],
                            placed,
                            plane,
                            &source,
                            &mut *page_pixels,
                            actors::colour(self.actor_ranges, semantic, self.team),
                        )?;
                    }
                }
            }
            if page_index == 0 {
                aoe_rendering::write_terrain_directory_page(self.art, page_pixels)
                    .map_err(error)?;
            }
            sink.commit(page_index).map_err(error)?;
        }
        sink.finish().map_err(error)
    }
}

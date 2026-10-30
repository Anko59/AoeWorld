//! Decode each source plane once, compose bounded scratch, then GPU-blit cells.
use super::{placement::Plane, streaming::Input, *};
use std::collections::BTreeMap;

impl Input<'_> {
    pub(super) async fn upload_sources(
        &self,
        sink: &mut dyn AtlasSink,
        pages: u32,
    ) -> Result<(), JsValue> {
        let order = copy_order(self.manifest, self.selected, self.placements);
        let mut start = 0;
        while start < order.len() {
            let page_index = self.manifest.frames[self.selected[order[start]]].page;
            let atlas = &self.manifest.pages[usize::from(page_index)];
            let mut end = start + 1;
            while end < order.len()
                && self.manifest.frames[self.selected[order[end]]].page == page_index
            {
                end += 1;
            }
            let colour = page(&atlas.color).await?;
            let player = page(&atlas.player).await?;
            let shadow = page(&atlas.shadow).await?;
            let mut groups = BTreeMap::<_, Vec<usize>>::new();
            for &semantic in &order[start..end] {
                let f = &self.manifest.frames[self.selected[semantic]];
                groups
                    .entry((
                        !self.maskable[semantic],
                        actors::colour(self.actor_ranges, semantic, self.team),
                        f.source == "graphics.drs:[32, 112, 108, 115]:226",
                    ))
                    .or_default()
                    .push(semantic);
            }
            for ((unowned, team, _), group) in groups {
                let owned = !unowned;
                let mut output = vec![0; GAME_ATLAS_PAGE_BYTES];
                let mut mask = if owned {
                    vec![0; GAME_ATLAS_PAGE_BYTES]
                } else {
                    Vec::new()
                };
                let mut copies = Vec::with_capacity(group.len());
                for semantic in group {
                    let f = &self.manifest.frames[self.selected[semantic]];
                    let placed = self.placements[semantic];
                    let source = Placement {
                        page: 2,
                        x: f.x,
                        y: f.y,
                        width: f.width,
                        height: f.height,
                    };
                    for (plane, pixels) in [
                        (Plane::Color, &colour),
                        (
                            if owned {
                                Plane::PlayerCoverage
                            } else {
                                Plane::Player
                            },
                            &player,
                        ),
                        (Plane::Shadow, &shadow),
                    ] {
                        placement::copy_plane(f, source, plane, pixels, &mut output, team)?;
                    }
                    if owned {
                        let source = Placement { page: 5, ..source };
                        placement::copy_plane(
                            f,
                            source,
                            Plane::MaskBase,
                            &colour,
                            &mut mask,
                            team,
                        )?;
                        placement::copy_plane(
                            f,
                            source,
                            Plane::MaskPlayer,
                            &player,
                            &mut mask,
                            team,
                        )?;
                    }
                    copies.push(aoe_rendering::AtlasSourceCopy {
                        source: [f.x, f.y],
                        destination: [placed.x, placed.y],
                        size: [placed.width, placed.height],
                        colour: placed.page,
                        mask: owned
                            .then(|| aoe_rendering::atlas_mask_page(u32::from(placed.page)) as u16),
                    });
                }
                let completion = sink
                    .write_source(&output, owned.then_some(mask.as_slice()), &copies)
                    .map_err(error)?;
                aoe_rendering::GameRenderer::atlas_source_ready(completion)
                    .await
                    .map_err(error)?;
            }
            start = end;
        }
        // Placements reserve the top two rows; headers cannot overwrite cells.
        for layer in 0..pages {
            if (5..8).contains(&layer) || (layer >= 9 && layer % 2 == 1) {
                continue;
            }
            let pixels = sink.page_buffer(layer).map_err(error)?;
            pixels[..4].copy_from_slice(&[255; 4]);
            if layer == 0 {
                aoe_rendering::write_terrain_directory_page(self.art, pixels).map_err(error)?;
            }
            sink.commit(layer).map_err(error)?;
        }
        Ok(())
    }
}

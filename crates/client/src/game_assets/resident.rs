//! CPU backends already retain the full atlas; fill it in source order so
//! each PNG is decoded once, including the aligned player-mask destination.
use super::{placement::Plane, streaming::Input, *};

impl Input<'_> {
    pub(super) async fn upload_resident(&self, pixels: &mut [u8]) -> Result<(), JsValue> {
        if aoe_rendering::atlas_pages(pixels.len()).is_none() {
            return Err(error("Invalid resident atlas size"));
        }
        for page in 0..pixels.len() / GAME_ATLAS_PAGE_BYTES {
            if (5..8).contains(&page) || (page >= 9 && page % 2 == 1) {
                continue;
            }
            let start = page * GAME_ATLAS_PAGE_BYTES;
            pixels[start..start + 4].copy_from_slice(&[255; 4]);
        }
        let order = copy_order(self.manifest, self.selected, self.placements);
        let mut start = 0;
        while start < order.len() {
            let source_page = self.manifest.frames[self.selected[order[start]]].page;
            let atlas = &self.manifest.pages[usize::from(source_page)];
            let mut end = start + 1;
            while end < order.len()
                && self.manifest.frames[self.selected[order[end]]].page == source_page
            {
                end += 1;
            }
            for (plane, name) in [
                (Plane::Color, &atlas.color),
                (Plane::Player, &atlas.player),
                (Plane::Shadow, &atlas.shadow),
            ] {
                let source = page(name).await?;
                for &semantic in &order[start..end] {
                    let placed = self.placements[semantic];
                    let owned = self.maskable[semantic];
                    let frame = &self.manifest.frames[self.selected[semantic]];
                    let team = actors::colour(self.actor_ranges, semantic, self.team);
                    placement::copy_plane(
                        frame,
                        placed,
                        if owned && matches!(plane, Plane::Player) {
                            Plane::PlayerCoverage
                        } else {
                            plane
                        },
                        &source,
                        pixels,
                        team,
                    )?;
                    let mask_plane = match plane {
                        Plane::Color => Plane::MaskBase,
                        Plane::Player => Plane::MaskPlayer,
                        _ => continue,
                    };
                    if owned {
                        placement::copy_plane(
                            frame,
                            Placement {
                                page: aoe_rendering::atlas_mask_page(u32::from(placed.page)) as u16,
                                ..placed
                            },
                            mask_plane,
                            &source,
                            pixels,
                            team,
                        )?;
                    }
                }
            }
            start = end;
        }
        aoe_rendering::write_terrain_directory_page(self.art, &mut pixels[..GAME_ATLAS_PAGE_BYTES])
            .map_err(error)
    }
}

//! Bounded native cell updates shared by every rendering tier.
use super::GameRenderer;

pub(in crate::game_renderer) fn validate(
    page: u32,
    origin: [u32; 2],
    size: [u32; 2],
    pixels: &[u8],
    pages: u32,
) -> Result<(), String> {
    if page >= pages
        || size.contains(&0)
        || (0..2).any(|axis| {
            u64::from(origin[axis]) + u64::from(size[axis]) > u64::from(crate::GAME_ATLAS_SIDE)
        })
        || u64::from(size[0]) * u64::from(size[1]) * 4 != pixels.len() as u64
    {
        return Err("Invalid atlas update bounds".into());
    }
    Ok(())
}

pub(in crate::game_renderer) fn copy(
    atlas: &mut [u8],
    page: u32,
    origin: [u32; 2],
    size: [u32; 2],
    pixels: &[u8],
) -> Result<(), String> {
    let pages = crate::atlas_pages(atlas.len()).ok_or("Invalid resident atlas size")?;
    validate(page, origin, size, pixels, pages)?;
    for row in 0..size[1] as usize {
        let from = row * size[0] as usize * 4;
        let to = page as usize * crate::GAME_ATLAS_PAGE_BYTES
            + ((origin[1] as usize + row) * crate::GAME_ATLAS_SIDE as usize + origin[0] as usize)
                * 4;
        atlas[to..to + size[0] as usize * 4]
            .copy_from_slice(&pixels[from..from + size[0] as usize * 4]);
    }
    Ok(())
}

impl GameRenderer {
    /// Validate a WebGL upload burst once before accepting its result. CPU
    /// bounds checks and each backend's queue admission still run per patch.
    /// Finish even after a closure error so the next burst starts cleanly.
    pub fn with_game_atlas_updates<T, E: From<String>>(
        &mut self,
        update: impl FnOnce(&mut Self) -> Result<T, E>,
    ) -> Result<T, E> {
        if let Self::WebGl(renderer) = self {
            renderer.begin_patches().map_err(E::from)?;
        }
        let result = update(self);
        let finished = match self {
            Self::WebGl(renderer) => renderer.finish_patches().map_err(E::from),
            _ => Ok(()),
        };
        result.and_then(|value| finished.map(|()| value))
    }

    /// Native uploads share the two-frame queue bound with rendering.
    pub fn atlas_updates_ready(&self) -> Result<bool, String> {
        match self {
            Self::WebGpu(renderer) => renderer.atlas_updates_ready(),
            _ => Ok(true),
        }
    }
    /// Updates an existing layer; geometry and the allocated layer budget stay fixed.
    pub fn update_game_atlas(
        &mut self,
        page: u32,
        origin: [u32; 2],
        size: [u32; 2],
        pixels: &[u8],
    ) -> Result<(), String> {
        match self {
            Self::WebGpu(renderer) => {
                validate(
                    page,
                    origin,
                    size,
                    pixels,
                    renderer._atlas.depth_or_array_layers(),
                )?;
                renderer.health.allow_upload()?;
                renderer.queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        origin: wgpu::Origin3d {
                            x: origin[0],
                            y: origin[1],
                            z: page,
                        },
                        ..renderer._atlas.as_image_copy()
                    },
                    pixels,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(size[0] * 4),
                        rows_per_image: Some(size[1]),
                    },
                    wgpu::Extent3d {
                        width: size[0],
                        height: size[1],
                        depth_or_array_layers: 1,
                    },
                );
                renderer.health.submitted(&renderer.queue, true);
                Ok(())
            }
            Self::WebGl(renderer) => renderer.update(page, origin, size, pixels),
            Self::Canvas {
                source_atlas,
                atlas,
                presentation,
                ..
            } => {
                copy(source_atlas, page, origin, size, pixels)?;
                *atlas = [None, None, None, None];
                presentation.invalidate();
                Ok(())
            }
        }
    }
}

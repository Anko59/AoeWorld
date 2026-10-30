//! Source-order GPU blits: one reusable two-plane texture, bounded submissions.
use super::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

#[derive(Clone, Copy, Debug)]
pub struct AtlasSourceCopy {
    pub source: [u16; 2],
    pub destination: [u16; 2],
    pub size: [u16; 2],
    pub colour: u16,
    pub mask: Option<u16>,
}

impl GameRenderer {
    /// Compose a source once on the CPU, then blit its rectangles directly into
    /// their resident layers. The returned completion must be awaited before
    /// another batch, bounding driver staging to two 16 MiB source planes.
    pub fn write_game_atlas_source(
        &mut self,
        upload: &mut AtlasUpload,
        colour: &[u8],
        mask: Option<&[u8]>,
        copies: &[AtlasSourceCopy],
    ) -> Result<js_sys::Promise, String> {
        let invalid = copies.is_empty()
            || copies.len() > 65536
            || colour.len() != crate::GAME_ATLAS_PAGE_BYTES
            || mask.is_some_and(|m| m.len() != crate::GAME_ATLAS_PAGE_BYTES)
            || copies.iter().any(|c| {
                let mask_colour =
                    (5..8).contains(&c.colour) || (c.colour >= 9 && c.colour % 2 == 1);
                u32::from(c.colour) >= upload.1
                    || mask_colour
                    || c.destination.iter().any(|&v| v < 2)
                    || (0..2).any(|i| {
                        c.size[i] == 0
                            || u32::from(c.source[i]) + u32::from(c.size[i])
                                > crate::GAME_ATLAS_SIDE
                            || u32::from(c.destination[i]) + u32::from(c.size[i])
                                > crate::GAME_ATLAS_SIDE
                    })
                    || c.mask.is_some_and(|m| {
                        mask.is_none()
                            || c.colour < 2
                            || u32::from(m) >= upload.1
                            || u32::from(m) != crate::atlas_mask_page(u32::from(c.colour))
                    })
            });
        if invalid {
            return Err("Invalid source atlas batch".into());
        }
        let (Self::WebGpu(renderer), Staging::Gpu(destination, _)) = (self, &upload.0) else {
            return Err("Source blits require WebGPU".into());
        };
        renderer.check_health()?;
        renderer.health.allow_upload()?;
        let source = upload.2.get_or_insert_with(|| {
            renderer.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("bounded source atlas planes"),
                size: wgpu::Extent3d {
                    width: crate::GAME_ATLAS_SIDE,
                    height: crate::GAME_ATLAS_SIDE,
                    depth_or_array_layers: 2,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            })
        });
        for (layer, pixels) in [(0, Some(colour)), (1, mask)] {
            let Some(pixels) = pixels else {
                continue;
            };
            renderer.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: layer,
                    },
                    ..source.as_image_copy()
                },
                pixels,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(crate::GAME_ATLAS_SIDE * 4),
                    rows_per_image: Some(crate::GAME_ATLAS_SIDE),
                },
                wgpu::Extent3d {
                    width: crate::GAME_ATLAS_SIDE,
                    height: crate::GAME_ATLAS_SIDE,
                    depth_or_array_layers: 1,
                },
            );
        }
        let mut encoder = renderer
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("source atlas blits"),
            });
        for c in copies {
            for (source_layer, target) in [(0, Some(c.colour)), (1, c.mask)] {
                let Some(target) = target else {
                    continue;
                };
                encoder.copy_texture_to_texture(
                    wgpu::TexelCopyTextureInfo {
                        origin: wgpu::Origin3d {
                            x: u32::from(c.source[0]),
                            y: u32::from(c.source[1]),
                            z: source_layer,
                        },
                        ..source.as_image_copy()
                    },
                    wgpu::TexelCopyTextureInfo {
                        origin: wgpu::Origin3d {
                            x: u32::from(c.destination[0]),
                            y: u32::from(c.destination[1]),
                            z: u32::from(target),
                        },
                        ..destination.as_image_copy()
                    },
                    wgpu::Extent3d {
                        width: u32::from(c.size[0]),
                        height: u32::from(c.size[1]),
                        depth_or_array_layers: 1,
                    },
                );
            }
        }
        renderer.queue.submit([encoder.finish()]);
        renderer.health.submitted(&renderer.queue, true);
        Ok(renderer
            .queue
            .as_webgpu()
            .ok_or("No browser GPU queue")?
            .on_submitted_work_done()
            .unchecked_into())
    }

    /// Match initialization's existing five-second device deadline; rejection
    /// and stalled queues remain errors instead of retaining unlimited batches.
    pub async fn atlas_source_ready(promise: js_sys::Promise) -> Result<(), String> {
        crate::web::health::within(async {
            JsFuture::from(promise)
                .await
                .map(|_| ())
                .map_err(|e| format!("Source atlas upload failed: {e:?}"))
        })
        .await
    }
}

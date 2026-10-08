use super::*;

#[wasm_bindgen_test]
async fn webgpu_restored_native_forest_soil_endpoints_gradient_seam_and_dirt_primary() {
    use crate::surface_mesh::floor as fixture;
    let mut renderer = surface_renderer_with_atlas(&fixture::forest_atlas()).await;
    for material in [0, 2, 6] {
        for (floor, gradient) in [(0, false), (650, false), (1000, false), (650, true)] {
            let mut faces = fixture::forest_faces(material, floor, gradient);
            for _ in 0..2 {
                renderer
                    .render_world_layers(&faces, &[], [0.0, 0.0, 0.0, 1.0])
                    .unwrap();
                let pixels = read_pixels(&renderer, 2, fixture::FOREST_PROBES).await;
                for ([x, _], pixel) in fixture::FOREST_PROBES.into_iter().zip(pixels) {
                    assert_pixel(
                        pixel,
                        fixture::forest_expected(material, floor, gradient, x),
                    );
                }
                faces.reverse();
            }
        }
    }
    renderer.device.destroy();
}

#[wasm_bindgen_test]
async fn webgpu_shared_floor_gradient_seam_and_old_packet_fallback() {
    use crate::surface_mesh::floor as fixture;
    let mut renderer = surface_renderer().await;
    for interpolated in [true, false] {
        let mut faces = fixture::faces(interpolated);
        for _ in 0..2 {
            renderer
                .render_world_layers(&faces, &[], [0.0, 0.0, 0.0, 1.0])
                .unwrap();
            let pixels = read_pixels(&renderer, 2, fixture::PROBES).await;
            for ([x, _], pixel) in fixture::PROBES.into_iter().zip(pixels) {
                assert_pixel(pixel, fixture::expected(x, interpolated));
            }
            faces.reverse();
        }
    }
    renderer.device.destroy();
}

#[wasm_bindgen_test]
async fn webgpu_crossfades_the_same_native_material_pixel_as_canvas() {
    let mut renderer = surface_renderer().await;
    check_surface_pixel(&mut renderer, 0, true, [82, 86, 86, 255]).await;
    renderer.device.destroy();
}

#[wasm_bindgen_test]
async fn webgpu_procedural_rock_snow_ice_mud_and_shore_match_shared_kernel() {
    // One device/atlas for every probe: context churn is not this pixel contract.
    let mut renderer = surface_renderer().await;
    for tint in (5..=10).chain([12]).chain(21..=26) {
        check_surface_pixel(
            &mut renderer,
            tint,
            false,
            crate::surface_mesh::procedural_tint([255, 0, 0, 255], tint),
        )
        .await;
    }
    renderer.device.destroy();
}

#[wasm_bindgen_test]
async fn webgpu_v2_floor_packets_use_uniform_three_page_pixels_without_geometry_changes() {
    let mut renderer = surface_renderer().await;
    let address = |page| crate::AtlasAddress {
        page,
        uv: [0.0, 0.0, 1.0 / 2048.0, 1.0 / 2048.0],
    };
    let mut face = capacity_surface();
    face.points = [[16.0, 16.0], [112.0, 16.0], [16.0, 112.0]].map(surface_point);
    face.texture_uv = Some(address(0));
    face.texture_blend = Some([address(1), address(2)]);
    assert_eq!(surface_instance(&face, [128.0; 2], 0.0).pages[3], 0);
    for palette in 0..4 {
        face.appearance =
            crate::surface_mesh::landscape::pack(Some(crate::SceneTerrainAppearance {
                floor_strength: 650,
                canopy_strength: 650,
                palette,
                exposure: 1,
                height_band: 2,
            }));
        face.tint = 1;
        let packet = surface_instance(&face, [128.0; 2], 0.0);
        assert_eq!(packet.pages[..3], [0, 1, 2]);
        assert_eq!(packet.pages[3] >> 29, 0);
        let expected = crate::surface_mesh::landscape::texel(
            [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]],
            crate::surface_mesh::landscape::floor_weights(packet.pages[3]),
            1,
            packet.pages[3],
        );
        assert_ne!(expected, [82, 86, 86, 255]);
        renderer
            .render_world_layers(&[face], &[], [0.0, 0.0, 0.0, 1.0])
            .unwrap();
        for pixel in read_pixels(&renderer, 1, [[48, 48], [32, 32]]).await {
            assert_pixel(pixel, expected);
        }
    }
    renderer.device.destroy();
}

#[wasm_bindgen_test]
async fn webgpu_landscape_dirt_pixels_use_authoritative_primary_after_texture_assignment() {
    let mut renderer = surface_renderer().await;
    let frame = |page| crate::GameFrame {
        atlas: crate::AtlasAddress {
            page,
            uv: [0.0, 0.0, 1.0 / 2048.0, 1.0 / 2048.0],
        },
        size: [1.0; 2],
        anchor: [0.0; 2],
    };
    let mut art = crate::GameArt {
        walking: Vec::new(),
        standing: Vec::new(),
        grass: vec![frame(0)],
        terrain: std::array::from_fn(|_| vec![frame(0)]),
        terrain_topology: [None; 7],
        resources: std::array::from_fn(|_| Vec::new()),
        tree_shadows: Vec::new(),
        tree_families: Default::default(),
    };
    art.terrain[2] = vec![frame(1)];
    art.terrain[6] = vec![frame(2)];
    for palette in 0..6 {
        for floor_strength in [0, 650, 1000] {
            let mut face = capacity_surface();
            face.points = [[16.0, 16.0], [112.0, 16.0], [16.0, 112.0]].map(surface_point);
            face.material = 2;
            face.tint = 1;
            face.appearance =
                crate::surface_mesh::landscape::pack(Some(crate::SceneTerrainAppearance {
                    floor_strength,
                    canopy_strength: 650,
                    palette,
                    exposure: 1,
                    height_band: 2,
                }));
            crate::surface_mesh::apply_terrain_textures(std::slice::from_mut(&mut face), &art);
            let packet = surface_instance(&face, [128.0; 2], 0.0);
            assert_eq!(face.texture_uv, Some(frame(1).atlas));
            assert_eq!(face.texture_blend, Some([frame(2).atlas, frame(1).atlas]));
            assert_eq!(packet.pages[..3], [1, 2, 1]);
            let expected = crate::surface_mesh::landscape::texel(
                [[0, 255, 0, 255], [0, 0, 255, 255], [0, 255, 0, 255]],
                crate::surface_mesh::landscape::floor_weights(packet.pages[3]),
                1,
                packet.pages[3],
            );
            renderer
                .render_world_layers(&[face], &[], [0.0, 0.0, 0.0, 1.0])
                .unwrap();
            for pixel in read_pixels(&renderer, 1, [[48, 48], [32, 32]]).await {
                assert_pixel(pixel, expected);
            }
        }
    }
    renderer.device.destroy();
}

pub(super) async fn surface_renderer() -> Renderer {
    let mut atlas = vec![0; crate::GAME_ATLAS_BYTES];
    atlas[..12].copy_from_slice(&[255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255]);
    let page_bytes = crate::GAME_ATLAS_PAGE_BYTES;
    atlas[page_bytes..page_bytes + 4].copy_from_slice(&[0, 255, 0, 255]);
    atlas[page_bytes * 2..page_bytes * 2 + 12]
        .copy_from_slice(&[0, 0, 255, 255, 255, 255, 0, 255, 0, 0, 0, 128]);
    surface_renderer_with_atlas(&atlas).await
}

pub(super) async fn surface_renderer_with_atlas(atlas: &[u8]) -> Renderer {
    let document = web_sys::window().unwrap().document().unwrap();
    let canvas = document
        .create_element("canvas")
        .unwrap()
        .dyn_into::<HtmlCanvasElement>()
        .unwrap();
    canvas.set_width(128);
    canvas.set_height(128);
    let mut renderer = Renderer::new(canvas)
        .await
        .expect("software WebGPU renderer");
    let diagnostic = renderer.render_sprites(&[]).unwrap();
    assert_eq!(diagnostic.atlas_pages, 1);
    assert_eq!(diagnostic.atlas_bytes, 8 * 8 * 4);
    renderer.upload_game_atlas(atlas).unwrap();
    renderer
}

async fn check_surface_pixel(renderer: &mut Renderer, tint: u8, blend: bool, expected: [u8; 4]) {
    let rect = |x: f32| crate::AtlasAddress {
        page: 0,
        uv: [x / 2048.0, 0.0, 1.0 / 2048.0, 1.0 / 2048.0],
    };
    let mut triangle = capacity_surface();
    triangle.points = [[16.0, 16.0], [112.0, 16.0], [16.0, 112.0]].map(surface_point);
    triangle.texture_uv = Some(rect(0.0));
    triangle.tint = tint;
    triangle.texture_blend = blend.then_some([rect(1.0), rect(2.0)]);
    let instance = surface_instance(&triangle, [128.0; 2], 0.0);
    assert_eq!(instance.color[3], if blend { -3.0 } else { -1.0 });
    renderer
        .render_world_layers(&[triangle], &[], [0.0, 0.0, 0.0, 1.0])
        .unwrap();

    let pixel = read_pixel(renderer, 1, [48, 48]).await;
    assert_pixel(pixel, expected);
}

pub(super) fn assert_pixel(pixel: [u8; 4], expected: [u8; 4]) {
    for (actual, expected) in pixel.into_iter().zip(expected) {
        assert!(
            actual.abs_diff(expected) <= 1,
            "unexpected GPU blend pixel: {pixel:?}"
        );
    }
}

pub(super) async fn read_pixel(renderer: &Renderer, count: u32, point: [u32; 2]) -> [u8; 4] {
    read_pixel_with_clear(renderer, count, point, wgpu::Color::BLACK).await
}

pub(super) async fn read_pixel_with_clear(
    renderer: &Renderer,
    count: u32,
    point: [u32; 2],
    clear: wgpu::Color,
) -> [u8; 4] {
    read_pixels_with_clear(renderer, count, [point], clear).await[0]
}

pub(super) async fn read_pixels<const N: usize>(
    renderer: &Renderer,
    count: u32,
    points: [[u32; 2]; N],
) -> [[u8; 4]; N] {
    read_pixels_with_clear(renderer, count, points, wgpu::Color::BLACK).await
}

pub(super) async fn read_pixels_with_clear<const N: usize>(
    renderer: &Renderer,
    count: u32,
    points: [[u32; 2]; N],
    clear: wgpu::Color,
) -> [[u8; 4]; N] {
    assert!((1..=5).contains(&N), "bounded GPU readback probe count");
    assert!(points.iter().all(|p| p[0] < 128 && p[1] < 128));
    // One shared pass preserves instance/depth ordering for every requested pixel.
    // Disjoint 256-byte-aligned copies use at most 1280 bytes and one mapped buffer.
    // Read actual shader pixels without relying on compositor canvas lifetime.
    let size = wgpu::Extent3d {
        width: 128,
        height: 128,
        depth_or_array_layers: 1,
    };
    let target = renderer.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("splat pixel test"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: renderer.config.format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let output = renderer.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("splat pixel readback"),
        size: N as u64 * 256,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let view = target.create_view(&Default::default());
    let depth = renderer.depth.create_view(&Default::default());
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("splat test pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(clear),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        pass.set_pipeline(&renderer.pipeline);
        renderer.instances.set_on(&mut pass);
        pass.draw(0..6, 0..count);
    }
    for (index, point) in points.iter().enumerate() {
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &target,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: point[0],
                    y: point[1],
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &output,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: index as u64 * 256,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(1),
                },
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
    }
    renderer.queue.submit([encoder.finish()]);
    let ready = std::rc::Rc::new(std::cell::RefCell::new(None));
    let wake = std::rc::Rc::new(std::cell::RefCell::new(None::<std::task::Waker>));
    let (done, waiter) = (ready.clone(), wake.clone());
    output
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            // Resolve failures too: a panic inside the browser callback leaves
            // the awaited test pending forever and hides the actual map error.
            done.replace(Some(result));
            if let Some(waker) = waiter.borrow_mut().take() {
                waker.wake();
            }
        });
    std::future::poll_fn(|context| {
        if ready.borrow().is_some() {
            std::task::Poll::Ready(())
        } else {
            *wake.borrow_mut() = Some(context.waker().clone());
            std::task::Poll::Pending
        }
    })
    .await;
    ready
        .borrow_mut()
        .take()
        .expect("GPU map callback completed")
        .expect("GPU pixel readback map");
    let data = output
        .slice(..)
        .get_mapped_range()
        .expect("mapped GPU pixel bytes");
    let pixels = std::array::from_fn(|index| {
        let start = index * 256;
        let sample = &data[start..start + 4];
        match renderer.config.format {
            wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb => {
                [sample[2], sample[1], sample[0], sample[3]]
            }
            _ => [sample[0], sample[1], sample[2], sample[3]],
        }
    });
    drop(data);
    output.unmap();
    output.destroy();
    target.destroy();
    pixels
}

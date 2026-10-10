use super::*;

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
        assert_pixel(read_pixel(&renderer, 1, [48, 48]).await, expected);
        assert_pixel(read_pixel(&renderer, 1, [32, 32]).await, expected);
    }
    renderer.device.destroy();
}

#[wasm_bindgen_test]
async fn webgpu_landscape_parity_fixture_matches_shared_expected_texels() {
    use crate::surface_mesh::landscape::parity;
    let mut renderer = surface_renderer().await;
    renderer.upload_game_atlas(&parity::atlas()).unwrap();
    let mut face = capacity_surface();
    face.points = [[16.0, 16.0], [112.0, 16.0], [16.0, 112.0]].map(surface_point);
    for case in parity::cases() {
        let mut texels = Vec::new();
        for legacy in [true, false] {
            parity::apply(&case, &mut face, legacy);
            renderer
                .render_world_layers(&[face], &[], [0.0, 0.0, 0.0, 1.0])
                .unwrap();
            for at in [48, 32] {
                texels.push(read_pixel(&renderer, 1, [at, at]).await);
            }
        }
        for index in 0..2 {
            let expected = case.expected.unwrap_or(texels[index]);
            assert_pixel(texels[index + 2], expected);
        }
    }
    renderer.device.destroy();
}

pub(super) async fn surface_renderer() -> Renderer {
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
    let mut atlas = vec![0; (3 * crate::GAME_ATLAS_SIDE * crate::GAME_ATLAS_SIDE * 4) as usize];
    atlas[..12].copy_from_slice(&[255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255]);
    let page_bytes = (crate::GAME_ATLAS_SIDE * crate::GAME_ATLAS_SIDE * 4) as usize;
    atlas[page_bytes..page_bytes + 4].copy_from_slice(&[0, 255, 0, 255]);
    atlas[page_bytes * 2..page_bytes * 2 + 12]
        .copy_from_slice(&[0, 0, 255, 255, 255, 255, 0, 255, 0, 0, 0, 128]);
    renderer.upload_game_atlas(&atlas).unwrap();
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
    // Execute the same pipeline/instances into an explicit GPU attachment. A
    // buffer copy reads actual shader pixels without compositor canvas expiry.
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
        size: 256,
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
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
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
                offset: 0,
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
    renderer.queue.submit([encoder.finish()]);
    let ready = std::rc::Rc::new(std::cell::Cell::new(false));
    let wake = std::rc::Rc::new(std::cell::RefCell::new(None::<std::task::Waker>));
    let (done, waiter) = (ready.clone(), wake.clone());
    output
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            result.expect("GPU pixel readback map");
            done.set(true);
            if let Some(waker) = waiter.borrow_mut().take() {
                waker.wake();
            }
        });
    std::future::poll_fn(|context| {
        if ready.get() {
            std::task::Poll::Ready(())
        } else {
            *wake.borrow_mut() = Some(context.waker().clone());
            std::task::Poll::Pending
        }
    })
    .await;
    let data = output
        .slice(..)
        .get_mapped_range()
        .expect("mapped GPU pixel bytes");
    let pixel = match renderer.config.format {
        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb => {
            [data[2], data[1], data[0], data[3]]
        }
        _ => [data[0], data[1], data[2], data[3]],
    };
    drop(data);
    output.unmap();
    output.destroy();
    target.destroy();
    pixel
}

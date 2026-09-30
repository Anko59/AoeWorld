use super::*;

#[wasm_bindgen_test]
async fn webgpu_crossfades_the_same_native_material_pixel_as_canvas() {
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
    let mut atlas = vec![0; (crate::GAME_ATLAS_SIDE * crate::GAME_ATLAS_SIDE * 4) as usize];
    atlas[..12].copy_from_slice(&[255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255]);
    renderer.upload_game_atlas(&atlas).unwrap();
    let rect = |x: f32| [x / 2048.0, 0.0, 1.0 / 2048.0, 1.0 / 2048.0];
    let mut triangle = capacity_surface();
    triangle.points = [[16.0, 16.0], [112.0, 16.0], [16.0, 112.0]].map(surface_point);
    triangle.texture_uv = Some(rect(0.0));
    triangle.texture_blend = Some([rect(1.0), rect(2.0)]);
    let instance = surface_instance(&triangle, [128.0; 2], 0.0);
    assert_eq!(instance.color[3], -3.0);
    assert_eq!(instance.terrain_blend, triangle.texture_blend.unwrap());
    renderer
        .render_world_layers(&[triangle], &[], [0.0, 0.0, 0.0, 1.0])
        .unwrap();

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
        pass.draw(0..6, 0..1);
    }
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &target,
            mip_level: 0,
            origin: wgpu::Origin3d { x: 48, y: 48, z: 0 },
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
    for (actual, expected) in pixel.into_iter().zip([82_u8, 86, 86, 255]) {
        assert!(
            actual.abs_diff(expected) <= 1,
            "unexpected GPU blend pixel: {pixel:?}"
        );
    }
    drop(data);
    output.unmap();
    output.destroy();
    target.destroy();
}

use super::*;
use crate::{GameArt, GameRenderer, SceneCamera, SceneTerrain, SceneTerrainSurface, game_grid};

#[wasm_bindgen_test]
async fn webgpu_live_source_grid_has_pixels_and_toggle_off_removes_them() {
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
    let cleared = renderer
        .render_sprites(&[])
        .expect("clear-only presentation");
    assert!(
        cleared.did_present,
        "an empty submitted clear is still visible"
    );
    assert_eq!(cleared.draw_calls, 0);
    let mut game = GameRenderer::WebGpu(Box::new(renderer));
    game.upload_game_atlas(&vec![
        255;
        (crate::GAME_ATLAS_SIDE * crate::GAME_ATLAS_SIDE * 4)
            as usize
    ])
    .unwrap();
    let camera = SceneCamera {
        center: [35_000.0; 2],
        zoom: 1.0,
        viewport: [128.0; 2],
        focus_elevation_meters: 800.0,
    };
    let terrain = [SceneTerrain {
        position: camera.center,
        material: 0,
        elevation_meters: 800.0,
        surface: SceneTerrainSurface::flat(800.0),
    }];
    let art = GameArt {
        walking: Vec::new(),
        standing: Vec::new(),
        grass: Vec::new(),
        terrain: std::array::from_fn(|_| Vec::new()),
        resources: std::array::from_fn(|_| Vec::new()),
        tree_shadows: Vec::new(),
    };
    let projection = aoe_core::Camera {
        center: camera.center,
        zoom: camera.zoom,
        viewport: camera.viewport,
        focus_elevation_meters: camera.focus_elevation_meters,
    };
    let config = aoe_core::WorldConfig::new(70_000, 70_000, aoe_core::Seed(0)).unwrap();
    let bounds = projection.visible_tiles_at_height(config, 1.0, 800.0);
    let count = game_grid::grid_sprites(camera, bounds).len();
    assert!(count > 0);
    let mut pixels = Vec::new();
    for enabled in [false, true, false] {
        game.render_prepared_world(
            &art,
            &terrain,
            &[],
            &[],
            &[],
            camera,
            0,
            enabled.then_some(bounds),
        )
        .unwrap();
        let GameRenderer::WebGpu(renderer) = &game else {
            unreachable!()
        };
        pixels.push(gpu_pixel(renderer, if enabled { count } else { 0 }).await);
    }
    assert_ne!(pixels[0], pixels[1]);
    assert_eq!(pixels[0], pixels[2]);
}

// Read production instance data with the real pipeline into a GPU attachment.
async fn gpu_pixel(renderer: &Renderer, count: usize) -> [u8; 4] {
    let target = renderer.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("grid pixels"),
        size: wgpu::Extent3d {
            width: 128,
            height: 128,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: renderer.config.format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let output = renderer.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("grid readback"),
        size: 256,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let view = target.create_view(&Default::default());
    let depth = renderer.depth.create_view(&Default::default());
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
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
        pass.draw(0..6, 0..count as u32);
    }
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &target,
            mip_level: 0,
            origin: wgpu::Origin3d { x: 64, y: 64, z: 0 },
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
            result.unwrap();
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
    let data = output.slice(..).get_mapped_range().unwrap();
    let pixel = [data[0], data[1], data[2], data[3]];
    drop(data);
    output.unmap();
    output.destroy();
    target.destroy();
    pixel
}

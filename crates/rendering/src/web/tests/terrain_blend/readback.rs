//! Bounded per-test GPU readback resources, never shared across renderers.
use super::*;

pub(super) struct PixelReadback<const N: usize> {
    target: wgpu::Texture,
    output: wgpu::Buffer,
    view: wgpu::TextureView,
    // This view retains the existing renderer-owned attachment, not another
    // depth texture. The renderer remains its sole destruction owner.
    depth: wgpu::TextureView,
    format: wgpu::TextureFormat,
}

impl<const N: usize> PixelReadback<N> {
    pub(super) fn new(renderer: &Renderer) -> Self {
        assert!((1..=5).contains(&N), "bounded GPU readback probe count");
        let target = renderer.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("splat pixel test"),
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
            label: Some("splat pixel readback"),
            size: N as u64 * 256,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let view = target.create_view(&Default::default());
        let depth = renderer.depth.create_view(&Default::default());
        Self {
            target,
            output,
            view,
            depth,
            format: renderer.config.format,
        }
    }

    // &mut self prevents overlapping maps. Every successful read unmaps before
    // returning, so subsequent copies only target a fully available buffer.
    pub(super) async fn read(
        &mut self,
        renderer: &Renderer,
        count: u32,
        points: [[u32; 2]; N],
        clear: wgpu::Color,
    ) -> [[u8; 4]; N] {
        assert!((1..=5).contains(&N), "bounded GPU readback probe count");
        assert!(points.iter().all(|p| p[0] < 128 && p[1] < 128));
        assert_eq!(renderer.config.format, self.format);
        let mut encoder = renderer.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("splat test pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
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
        // All original points share the same pass/depth/instance ordering.
        // Separate aligned copies occupy at most 1280 mapped bytes.
        for (index, point) in points.iter().enumerate() {
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.target,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: point[0],
                        y: point[1],
                        z: 0,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &self.output,
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
        self.output
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                // Resolve failures too: callback panics would leave the awaited
                // test pending forever instead of reporting its actual map error.
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
        let data = self
            .output
            .slice(..)
            .get_mapped_range()
            .expect("mapped GPU pixel bytes");
        let pixels = std::array::from_fn(|index| {
            let start = index * 256;
            let sample = &data[start..start + 4];
            match self.format {
                wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb => {
                    [sample[2], sample[1], sample[0], sample[3]]
                }
                _ => [sample[0], sample[1], sample[2], sample[3]],
            }
        });
        drop(data);
        self.output.unmap();
        pixels
    }
}

impl<const N: usize> Drop for PixelReadback<N> {
    fn drop(&mut self) {
        self.output.destroy();
        self.target.destroy();
        // Views drop normally; never destroy renderer.depth here.
    }
}

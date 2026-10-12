//! Relative GPU work budget: ordinary sprites must remain a one-read kernel.
//! Alternating on the same device avoids an absolute hosted-machine FPS claim.
use super::*;
use std::{
    sync::{Arc, Mutex},
    task::{Poll, Waker},
};

async fn drained(queue: &wgpu::Queue) {
    let done = Arc::new(Mutex::new((false, None::<Waker>)));
    let ready = done.clone();
    queue.on_submitted_work_done(move || {
        let waker = {
            let mut done = ready.lock().unwrap();
            done.0 = true;
            done.1.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    });
    std::future::poll_fn(|context| {
        let mut done = done.lock().unwrap();
        if done.0 {
            Poll::Ready(())
        } else {
            done.1 = Some(context.waker().clone());
            Poll::Pending
        }
    })
    .await;
}

fn reference_pipeline(renderer: &Renderer) -> wgpu::RenderPipeline {
    // Current production vertex ABI, with an independent one-read sprite oracle.
    // This is not a saved prior terrain shader or a compatibility implementation.
    let vertex = include_str!("../../sprites.wgsl")
        .split("@fragment")
        .next()
        .unwrap();
    let source = format!(
        "const ground_mixing: bool = false;\n{vertex}\n@fragment\nfn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {{\n let texel = textureSampleLevel(sprite_atlas, sprite_sampler, in.uv, i32(in.pages.x), 0.0);\n if texel.a <= 0.0 {{ discard; }}\n return texel * in.color;\n}}\n"
    );
    let shader = renderer
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("one-read sprite oracle"),
            source: wgpu::ShaderSource::Wgsl(Cow::Owned(source)),
        });
    let layout = renderer
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sprite cost comparison"),
            bind_group_layouts: &[Some(&renderer.pipeline.get_bind_group_layout(0))],
            immediate_size: 0,
        });
    renderer
        .device
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("one-read sprite oracle"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: renderer.config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: Default::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24Plus,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        })
}

#[wasm_bindgen_test]
async fn dense_ordinary_sprites_keep_the_one_read_fragment_cost() {
    let document = web_sys::window().unwrap().document().unwrap();
    let canvas: HtmlCanvasElement = document
        .create_element("canvas")
        .unwrap()
        .dyn_into()
        .unwrap();
    canvas.set_width(1920);
    canvas.set_height(1080);
    document.body().unwrap().append_child(&canvas).unwrap();
    let mut renderer = Renderer::new(canvas.clone()).await.unwrap();
    let production = renderer.pipeline.clone();
    let reference = reference_pipeline(&renderer);
    let sprites = (0..10_000)
        .map(|index| Sprite {
            position: [
                ((index % 128) as f32 * 4.0 + 12.0) / 1920.0 * 2.0 - 1.0,
                1.0 - ((index / 128) as f32 * 4.0 + 12.0) / 1080.0 * 2.0,
            ],
            radius: [12.0 / 1920.0, 12.0 / 1080.0],
            color: [1.0; 4],
            uv: [0.0, 0.0, 1.0, 1.0],
            depths: [0.0; 4],
            terrain_blend: [[0.0; 4]; 2],
            pages: [0; 4],
        })
        .collect::<Vec<_>>();
    let clock = js_sys::Reflect::get(&js_sys::global(), &"performance".into()).unwrap();
    let now: js_sys::Function = js_sys::Reflect::get(&clock, &"now".into())
        .unwrap()
        .dyn_into()
        .unwrap();
    let time = || now.call0(&clock).unwrap().as_f64().unwrap();
    let mut samples = [Vec::new(), Vec::new()];
    // Warm both driver programs, then alternate; drain the GPU, not just submit.
    for iteration in 0..6 {
        for (index, pipeline) in [&production, &reference].into_iter().enumerate() {
            renderer.pipeline = pipeline.clone();
            let start = time();
            let counters = renderer.render_sprites(&sprites).unwrap();
            assert!(counters.did_present);
            assert_eq!(counters.visible, 10_000);
            drained(&renderer.queue).await;
            if iteration >= 2 {
                samples[index].push(time() - start);
            }
        }
    }
    renderer.device.destroy();
    canvas.remove();
    let minimum = samples.map(|values| values.into_iter().fold(f64::INFINITY, f64::min));
    wasm_bindgen_test::console_log!(
        "10,000 sprite completed-frame ms: production={}, one-read={}, samples={:?}",
        minimum[0],
        minimum[1],
        minimum
    );
    // Same work should cost the same. Allow scheduling/clock overhead (5 ms)
    // and a factor of two, but reject terrain-program costs on every sprite.
    assert!(
        minimum[0] <= minimum[1] * 2.0 + 5.0,
        "ordinary sprite fragment regression: {} ms versus {} ms",
        minimum[0],
        minimum[1]
    );
}

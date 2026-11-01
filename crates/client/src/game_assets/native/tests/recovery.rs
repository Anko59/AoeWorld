use super::*;
use wasm_bindgen::JsCast;

#[wasm_bindgen(inline_js = "
let originalFetch, calls;
const losses = new WeakMap();
export function retry_fetch(bytes,abort) {
 const payload=bytes.slice();
 originalFetch = window.fetch; calls = 0;
 window.fetch = async () => {
  if(++calls===1){if(abort)throw new DOMException('timed out','AbortError');return new Response('', {status:503});}
  return new Response(payload.slice(), {headers:{'Content-Length':String(payload.length)}});
 };
}
export function restore_fetch() { window.fetch = originalFetch; return calls; }
export function lose_during_patch(canvas) {
 const gl=canvas.getContext('webgl2'), ext=gl.getExtension('WEBGL_lose_context');
 if(!ext) throw Error('context loss extension unavailable');
 losses.set(canvas,{ext,lost:new Promise((resolve,reject)=>canvas.addEventListener('webglcontextlost',event=>{
  if(event.defaultPrevented)resolve();else reject(Error('renderer did not allow context restoration'));
 },{once:true}))});
 const patch=gl.texSubImage3D.bind(gl); let once=true;
 gl.texSubImage3D=(...args)=>{if(once){once=false;ext.loseContext();}else patch(...args);};
}
export async function restore_native_context(canvas) {
 const loss=losses.get(canvas);await loss.lost;
 await new Promise((resolve,reject)=>{
  const timer=setTimeout(()=>reject(Error('context restoration timeout')),5000);
  canvas.addEventListener('webglcontextrestored',()=>{clearTimeout(timer);resolve();},{once:true});
  // Leave the loss event task before asking the browser to restore it.
  setTimeout(()=>loss.ext.restoreContext(),50);
 });
}
export function recovery_pixel(canvas,gl) {
 if(!gl) return canvas.getContext('2d').getImageData(64,64,1,1).data;
 const context=canvas.getContext('webgl2'),pixel=new Uint8Array(4);
 context.finish();context.readPixels(64,63,1,1,context.RGBA,context.UNSIGNED_BYTE,pixel);
 if(context.getError()!==context.NO_ERROR)throw Error('recovery pixel read failed');return pixel;
}
")]
extern "C" {
    fn retry_fetch(bytes: &[u8], abort: bool);
    fn restore_fetch() -> u32;
    fn lose_during_patch(canvas: &web_sys::HtmlCanvasElement);
    #[wasm_bindgen(catch)]
    async fn restore_native_context(
        canvas: &web_sys::HtmlCanvasElement,
    ) -> Result<JsValue, JsValue>;
    fn recovery_pixel(canvas: &web_sys::HtmlCanvasElement, gl: bool) -> Vec<u8>;
}

async fn wait(ms: i32) {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        web_sys::window()
            .unwrap()
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms)
            .unwrap();
    });
    wasm_bindgen_futures::JsFuture::from(promise).await.unwrap();
}

async fn fixture(
    backend: &str,
) -> (
    NativeFrames,
    GameArt,
    aoe_rendering::GameRenderer,
    web_sys::HtmlCanvasElement,
    Vec<Vec<u8>>,
) {
    let bytes: Vec<_> = [[255, 0, 0, 255], [0, 255, 0, 255]]
        .iter()
        .enumerate()
        .map(|(i, color)| {
            let player = if backend == "webgl2" && i == 1 {
                [7, 0, 0, 255]
            } else {
                [0; 4]
            };
            compressed::encode_frame([1, 1], [color.as_slice(), &player, &[0; 4]]).unwrap()
        })
        .collect();
    let chunks = bytes
        .iter()
        .enumerate()
        .map(|(i, b)| FrameChunk::from_bytes(format!("frames-{i:04}.bin"), b).unwrap())
        .collect();
    let cells = bytes
        .iter()
        .enumerate()
        .map(|(i, b)| Cell {
            owned: backend == "webgl2" && i == 1,
            ..cell(i as u16, b.len() as u32)
        })
        .collect();
    let mut native = NativeFrames::new(chunks, cells, 8).unwrap();
    native.cache = FrameCache::new(8, 1, 10).unwrap();
    let document = web_sys::window().unwrap().document().unwrap();
    let canvas = document
        .create_element("canvas")
        .unwrap()
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .unwrap();
    canvas.set_width(128);
    canvas.set_height(128);
    document.body().unwrap().append_child(&canvas).unwrap();
    let (mut renderer, canvas) =
        aoe_rendering::GameRenderer::new_with_backend(canvas, Some(backend))
            .await
            .unwrap();
    assert_eq!(
        renderer.backend(),
        if backend == "canvas" {
            "canvas2d"
        } else {
            backend
        }
    );
    let mut upload = renderer.begin_game_atlas(10).unwrap();
    for layer in 0..10 {
        upload.page_buffer(layer).unwrap();
        renderer.write_game_atlas_page(&mut upload, layer).unwrap();
    }
    renderer.finish_game_atlas(upload).unwrap();
    let mut art = super::runtime::empty_art();
    art.resident.resize(2, None);
    art.actor_rows = vec![16];
    native.requests(&[1]).unwrap();
    native.accept(0, bytes[0].clone()).unwrap();
    native
        .update(&[1], &[], &[], &mut art, &mut renderer)
        .unwrap();
    draw(&mut renderer, &mut art, 1);
    assert_eq!(
        recovery_pixel(&canvas, backend == "webgl2"),
        [255, 0, 0, 255]
    );
    (native, art, renderer, canvas, bytes)
}

fn draw(renderer: &mut aoe_rendering::GameRenderer, art: &mut GameArt, stream: u32) {
    art.actors = vec![vec![
        GameFrame {
            stream,
            atlas: AtlasAddress::WHITE,
            size: [32.0; 2],
            anchor: [16.0; 2]
        };
        16
    ]];
    let unit = aoe_rendering::SceneUnit {
        id: aoe_core::EntityId(1),
        kind: aoe_rendering::SceneUnitKind::actor(0),
        position: [0.0; 2],
        facing: 0,
        elevation_meters: 0.0,
        visual: aoe_rendering::SceneUnitVisual::new(false).with_owner(
            Some([65, 145, 245]),
            false,
            false,
        ),
    };
    assert!(
        renderer
            .render_world(
                art,
                &[],
                &[],
                &[unit],
                aoe_rendering::SceneCamera {
                    center: [0.0; 2],
                    zoom: 1.0,
                    viewport: [128.0; 2],
                    focus_elevation_meters: 0.0
                },
                0,
                false
            )
            .unwrap()
    );
}

#[wasm_bindgen_test]
async fn native_failed_then_successful_fetch_resumes_streaming_and_presentation() {
    for abort in [false, true] {
        fetch_recovers(abort).await;
    }
}

async fn fetch_recovers(abort: bool) {
    let (mut native, mut art, mut renderer, canvas, bytes) = fixture("canvas").await;
    retry_fetch(&bytes[1], abort);
    let (_, requests) = native
        .update(&[2], &[], &[1], &mut art, &mut renderer)
        .unwrap();
    assert_eq!(requests.len(), 1);
    for request in requests {
        request.spawn();
    }
    for _ in 0..100 {
        if !native.completed.borrow().is_empty() {
            break;
        }
        wait(10).await;
    }
    let result = native.update(&[2], &[], &[1], &mut art, &mut renderer);
    assert!(
        result.is_ok(),
        "one HTTP failure must not latch native streaming off"
    );
    assert!(result.unwrap().1.is_empty(), "retry must wait for backoff");
    assert!(!native.failed());
    assert!(native.ready());
    assert!(native.inflight.is_empty());
    assert_eq!(
        recovery_pixel(&canvas, false),
        [255, 0, 0, 255],
        "keep the last complete frame"
    );
    wait(300).await;
    let (_, requests) = native
        .update(&[2], &[], &[1], &mut art, &mut renderer)
        .unwrap();
    assert_eq!(requests.len(), 1);
    for request in requests {
        request.spawn();
    }
    for _ in 0..100 {
        if !native.completed.borrow().is_empty() {
            break;
        }
        wait(10).await;
    }
    native
        .update(&[2], &[], &[1], &mut art, &mut renderer)
        .unwrap();
    assert_eq!(restore_fetch(), 2);
    assert_eq!(native.retries[1].until, 0.0);
    assert_eq!(native.retries[1].attempts, 0);
    assert!(!native.failed());
    assert!(native.ready());
    assert!(art.resident[1].is_some());
    draw(&mut renderer, &mut art, 2);
    assert_eq!(recovery_pixel(&canvas, false), [0, 255, 0, 255]);
    canvas.remove();
}

#[wasm_bindgen_test]
fn native_retry_backoff_is_capped() {
    let mut retry = Retry::default();
    let mut now = 0.0;
    for delay in [250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 8000.0, 8000.0] {
        retry.fail(now);
        assert_eq!(retry.until, now + delay);
        assert!(retry.attempts <= 5);
        now = retry.until;
    }
}

#[wasm_bindgen_test]
async fn native_failed_fetch_releases_reservations_without_losing_other_completions() {
    let (native, mut art, mut renderer, canvas, bytes) = fixture("canvas").await;
    let mut native = NativeFrames::new(native.chunks.clone(), native.cells.clone(), 8).unwrap();
    native.cache = FrameCache::new(8, 1, 10).unwrap();
    art.resident.fill(None);
    assert_eq!(native.requests(&[1, 2]).unwrap(), [0, 1]);
    native.completed.borrow_mut().extend([
        (0, Err(JsValue::from_str("timed out"))),
        (1, Ok(bytes[1].clone())),
    ]);
    let (_, requests) = native
        .update(&[2], &[1], &[], &mut art, &mut renderer)
        .unwrap();
    assert!(requests.is_empty());
    assert!(native.inflight.is_empty());
    assert_eq!(native.memory_usage(&art)[1], 0);
    assert!(native.encoded[1].is_some());
    assert!(art.resident[1].is_some());
    assert!(!native.failed());
    canvas.remove();
}

#[wasm_bindgen_test]
async fn native_context_loss_during_patch_restores_resident_cells_and_presentation() {
    let (mut native, mut art, mut renderer, canvas, bytes) = fixture("webgl2").await;
    native.requests(&[2]).unwrap();
    native.accept(1, bytes[1].clone()).unwrap();
    lose_during_patch(&canvas);
    assert!(
        native
            .update(&[2], &[], &[1], &mut art, &mut renderer)
            .is_ok(),
        "context loss during a native patch must not be fatal"
    );
    assert!(!native.failed());
    assert!(art.resident[1].is_some());
    restore_native_context(&canvas).await.unwrap();
    draw(&mut renderer, &mut art, 1);
    assert_eq!(
        recovery_pixel(&canvas, true),
        [255, 0, 0, 255],
        "restore the previously resident cell"
    );
    draw(&mut renderer, &mut art, 2);
    assert_eq!(
        recovery_pixel(&canvas, true),
        [65, 145, 245, 255],
        "restore the interrupted native patch"
    );
    canvas.remove();
}

//! One bounded startup snapshot catches blank canvases despite queue success.
use wasm_bindgen::{JsCast, prelude::wasm_bindgen};
use wasm_bindgen_futures::JsFuture;
use web_sys::HtmlCanvasElement;

pub(super) const CLEAR: [f64; 4] = [0.16, 0.29, 0.14, 1.0];

// These are browser presentation operations. Keeping their promise chain in JS
// avoids Rust future/DOM conversion monomorphisations. Copy expected pixels
// before awaiting: never retain a borrowed view of WASM memory across a yield.
#[wasm_bindgen(inline_js = "
let retired = false;
export function page_retired() { return retired; }
export function retire_with_page(device) {
  const retire = event => {
    if (!event.persisted) { retired = true; device.destroy(); }
  };
  window.addEventListener('pagehide', retire);
  return () => window.removeEventListener('pagehide', retire);
}
export async function presentation_check(canvas, expected) {
  const clear = Array.from(expected);
  let bitmap;
  try {
    const blob = await new Promise((resolve, reject) => {
      try { canvas.toBlob(resolve); } catch (error) { reject(error); }
    });
    bitmap = await createImageBitmap(blob);
    const sample = document.createElement('canvas');
    sample.width = sample.height = 8;
    const context = sample.getContext('2d');
    if (!context) throw new Error('No presentation sample context');
    context.drawImage(bitmap, 0, 0, 8, 8);
    const pixels = context.getImageData(0, 0, 8, 8).data;
    for (let i = 0; i < pixels.length; i++) {
      if (Math.abs(pixels[i] - clear[i % 4]) > 2)
        throw new Error('WebGPU canvas did not display its startup clear');
    }
  } catch (error) {
    throw new Error('WebGPU presentation snapshot: ' + String(error));
  } finally {
    bitmap?.close();
  }
}
")]
extern "C" {
    pub(super) fn page_retired() -> bool;
    pub(super) fn retire_with_page(device: &wasm_bindgen::JsValue) -> js_sys::Function;
    fn presentation_check(canvas: HtmlCanvasElement, expected: &[u8]) -> js_sys::Promise;
}

pub(super) async fn check(canvas: HtmlCanvasElement) -> Result<(), String> {
    let expected = CLEAR.map(|channel| (channel * 255.0).round() as u8);
    JsFuture::from(presentation_check(canvas, &expected))
        .await
        .map(|_| ())
        .map_err(|error| {
            error.dyn_ref::<js_sys::Error>().map_or_else(
                || "WebGPU presentation snapshot failed".into(),
                |e| e.message().into(),
            )
        })
}

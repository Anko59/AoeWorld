//! Each browser app has its own current bundle; numeric presentation is shared.
#[path = "web/numeric.rs"]
pub(crate) mod numeric;

#[cfg(feature = "diagnostics")]
#[path = "web/lab.rs"]
mod lab;

#[cfg(not(feature = "diagnostics"))]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    // wasm32 panics abort silently; every later callback then traps too.
    // Report the first panic's message and stack, and tell the player.
    std::panic::set_hook(Box::new(|info| {
        let message = info.to_string();
        let stack =
            js_sys::Reflect::get(&js_sys::Error::new("panic"), &"stack".into()).unwrap_or_default();
        web_sys::console::error_2(&format!("AoeWorld client panic: {message}").into(), &stack);
        if let Some(status) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("connection"))
        {
            status.set_text_content(Some("Client crashed; reload"));
        }
    }));
    wasm_bindgen_futures::spawn_local(async {
        let result = async {
            let document = web_sys::window()
                .and_then(|w| w.document())
                .ok_or("No document")?;
            crate::playground::initialize(document).await
        }
        .await;
        if let Err(error) = result {
            if let Some(document) = web_sys::window().and_then(|w| w.document()) {
                if let Some(status) = document.get_element_by_id("connection") {
                    status.set_text_content(Some("Unavailable"));
                }
                if let Some(message) = document.get_element_by_id("unsupported") {
                    message.set_text_content(Some(&format!(
                        "Unable to start AoeWorld: {error:?}. Reload to retry."
                    )));
                }
            }
        }
    });
}

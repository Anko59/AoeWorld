//! GPU programs are generated assets, loaded only when WebGPU is requested.
use std::sync::OnceLock;
use wasm_bindgen::prelude::*;

static SOURCES: OnceLock<[String; 2]> = OnceLock::new();

#[wasm_bindgen(module = "/src/web/shaders.js")]
extern "C" {
    #[wasm_bindgen(catch)]
    async fn fetch_shaders() -> Result<js_sys::Array, JsValue>;
}

pub(super) fn validate(sources: &[String; 2]) -> Result<(), String> {
    for source in sources {
        if source.len() > 65536
            || !source.contains("fn vs_main(")
            || !source.contains("fn fs_main(")
            || !source.contains("fn fs_unit_contour(")
        {
            return Err("invalid WebGPU shader asset".into());
        }
    }
    Ok(())
}

pub(super) fn source(mixed: bool) -> &'static str {
    #[cfg(any(test, feature = "shader-source"))]
    SOURCES.get_or_init(|| crate::shader_data::browser_shaders().map(str::to_owned));
    // `load` installs validated sources before any pipeline is created. An
    // unloaded module stays invalid WGSL and follows renderer error recovery.
    SOURCES
        .get()
        .map_or("", |sources| sources[usize::from(mixed)].as_str())
}

pub(super) async fn load() -> Result<(), String> {
    #[cfg(any(test, feature = "shader-source"))]
    {
        source(false);
    }
    if SOURCES.get().is_some() {
        return Ok(());
    }
    let values = fetch_shaders()
        .await
        .map_err(|_| "WebGPU shader download failed")?;
    let sources = [0, 1].map(|i| values.get(i).as_string().unwrap_or_default());
    validate(&sources)?;
    let _ = SOURCES.set(sources);
    Ok(())
}

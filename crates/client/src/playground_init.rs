use super::{Client, animate, connect, controls, resize};
use crate::game_assets::load;
use aoe_core::{Camera, WorldConfig};
use aoe_rendering::GameRenderer;
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{Document, HtmlCanvasElement};

pub(super) async fn initialize(document: Document) -> Result<(), JsValue> {
    let canvas: HtmlCanvasElement = document
        .get_element_by_id("scene")
        .ok_or("No canvas")?
        .dyn_into()?;
    let (mut renderer, canvas) = GameRenderer::new(canvas)
        .await
        .map_err(|error| JsValue::from_str(&error))?;
    let (art, pixels) = load().await?;
    renderer
        .upload_game_atlas(&pixels)
        .map_err(|error| JsValue::from_str(&error))?;
    if let Some(element) = document.get_element_by_id("playground") {
        let _ = element.set_attribute("data-renderer", renderer.backend());
        let _ = element.set_attribute("data-assets", "aoe2-local");
    }
    let config = WorldConfig::default();
    let shared = Rc::new(RefCell::new(Client {
        document,
        canvas,
        renderer,
        art,
        socket: None,
        connection_id: 0,
        camera: Camera {
            center: [
                f64::from(config.width_tiles) / 2.0,
                f64::from(config.height_tiles) / 2.0,
            ],
            zoom: 1.0,
            viewport: [1.0, 1.0],
            focus_elevation_meters: 0.0,
        },
        config,
        primary: None,
        role: None,
        map_content_hash: None,
        focus_map_hash: None,
        terrain_chunks: BTreeMap::new(),
        terrain_inflight: Default::default(),
        token: super::storage::stored_token(),
        revision: 0,
        sent_region: None,
        last_subscribe: 0.0,
        next_sequence: 1,
        units: BTreeMap::new(),
        history: BTreeMap::new(),
        server_tick: 0,
        selected: None,
        grid: false,
        drag: None,
        pointer: None,
        last_frame: 0.0,
        status: "loading".to_owned(),
    }));
    resize(&mut shared.borrow_mut());
    controls::install(shared.clone())?;
    connect(shared.clone())?;
    animate(shared)?;
    Ok(())
}

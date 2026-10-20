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
    let backend = super::capture::current().renderer();
    let (mut renderer, canvas) = GameRenderer::new_with_backend(canvas, backend)
        .await
        .map_err(|error| JsValue::from_str(&error))?;
    let (art, pixels, colour) = load().await?;
    renderer
        .upload_game_atlas(&art, pixels)
        .map_err(|error| JsValue::from_str(&error))?;
    if let Some(element) = document.get_element_by_id("playground") {
        let _ = element.set_attribute("data-renderer", renderer.backend());
        let _ = element.set_attribute("data-assets", "aoe2-local");
        let _ = element.set_attribute("data-art-colour", &format!("{colour:?}"));
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
        capture: Default::default(),
        primary: None,
        role: None,
        map_content_hash: None,
        surface_fixture: false,
        focus_map_hash: None,
        resources: crate::resource_state::ResourceStateCache::default(),
        terrain_scene: RefCell::new(None),
        rendered_frame: super::map::render_cache::RenderFrameCache::default(),
        terrain_chunks: BTreeMap::new(),
        terrain_height_bounds: None,
        terrain_bounds: Default::default(),
        terrain_resident_height_bounds: None,
        terrain_requests: Default::default(),
        token: super::storage::stored_token(),
        revision: 0,
        sent_region: None,
        last_subscribe: 0.0,
        next_sequence: 1,
        order_feedback: Default::default(),
        units: BTreeMap::new(),
        presentation: Default::default(),
        selected: Default::default(),
        village: Default::default(),
        grid: false,
        drag: None,
        pointer: None,
        last_frame: 0.0,
        status: "loading".to_owned(),
    }));
    resize(&mut shared.borrow_mut());
    super::fixture::set_active(&shared);
    controls::install(shared.clone())?;
    super::village::hud::install(shared.clone())?;
    super::capture::install(shared.clone())?;
    connect(shared.clone())?;
    install_colour(shared.clone(), colour)?;
    animate(shared)?;
    Ok(())
}

// One serialized reload follows setup colour changes. Existing renderers keep
// the same fixed atlas allocation; no per-frame recolouring or extra pages.
fn install_colour(shared: Rc<RefCell<Client>>, colour: [u8; 3]) -> Result<(), JsValue> {
    use std::cell::Cell;
    use wasm_bindgen::closure::Closure;
    let loaded = Rc::new(Cell::new(colour));
    let busy = Rc::new(Cell::new(false));
    fn refresh(shared: Rc<RefCell<Client>>, loaded: Rc<Cell<[u8; 3]>>, busy: Rc<Cell<bool>>) {
        if busy.get() || loaded.get() == crate::game_assets::team_colour() {
            return;
        }
        busy.set(true);
        wasm_bindgen_futures::spawn_local(async move {
            loop {
                match load().await {
                    Ok((art, pixels, colour)) => {
                        let mut client = shared.borrow_mut();
                        if let Err(error) = client.renderer.upload_game_atlas(&art, pixels) {
                            client.status = error;
                            break;
                        }
                        if let Some(element) = client.document.get_element_by_id("playground") {
                            let _ =
                                element.set_attribute("data-art-colour", &format!("{colour:?}"));
                        }
                        client.art = art;
                        client.rendered_frame.clear();
                        loaded.set(colour);
                    }
                    Err(error) => {
                        shared.borrow_mut().status =
                            format!("Unable to update player colour: {error:?}");
                        break;
                    }
                }
                if loaded.get() == crate::game_assets::team_colour() {
                    break;
                }
            }
            busy.set(false);
        });
    }
    refresh(shared.clone(), loaded.clone(), busy.clone());
    let target = shared.clone();
    let callback = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
        refresh(target.clone(), loaded.clone(), busy.clone())
    });
    shared
        .borrow()
        .document
        .add_event_listener_with_callback("change", callback.as_ref().unchecked_ref())?;
    callback.forget();
    Ok(())
}

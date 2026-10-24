use super::{Client, animate, connect, controls, resize};
use crate::game_assets::{AtlasSink, RendererSink, load};
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
    let (art, colour) = load(&mut RendererSink::new(&mut renderer)).await?;
    if let Some(element) = document.get_element_by_id("playground") {
        let _ = element.set_attribute("data-renderer", renderer.backend());
        let _ = element.set_attribute("data-assets", "aoe2-local");
        let _ = element.set_attribute("data-art-colour", &format!("{colour:?}"));
    }
    let query = web_sys::window()
        .and_then(|w| w.location().search().ok())
        .and_then(|query| web_sys::UrlSearchParams::new_with_str(&query).ok());
    let hover_mode = match query.and_then(|query| query.get("hover")).as_deref() {
        Some("cursor") => 0,
        Some("glow") => 2,
        _ => 1,
    };
    if let Some(element) = document.get_element_by_id("playground") {
        let _ = element.set_attribute(
            "data-hover-mode",
            ["cursor", "outline", "glow"][hover_mode as usize],
        );
    }
    let config = WorldConfig::default();
    let shared = Rc::new(RefCell::new(Client {
        document,
        canvas,
        renderer,
        art,
        team_colour: colour,
        socket: None,
        connection_callbacks: None,
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
        fog: Default::default(),
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
        hover_mode,
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

struct SharedSink<'a> {
    shared: &'a Rc<RefCell<Client>>,
    upload: Option<aoe_rendering::AtlasUpload>,
}

impl<'a> SharedSink<'a> {
    fn new(shared: &'a Rc<RefCell<Client>>) -> Self {
        Self {
            shared,
            upload: None,
        }
    }
}

impl AtlasSink for SharedSink<'_> {
    fn resident_buffer(&mut self) -> Result<Option<&mut [u8]>, String> {
        if self.upload.is_none() {
            self.upload = Some(self.shared.borrow_mut().renderer.begin_game_atlas()?);
        }
        Ok(self
            .upload
            .as_mut()
            .ok_or("No atlas upload")?
            .resident_buffer())
    }
    fn page_buffer(&mut self, index: u32) -> Result<&mut [u8], String> {
        if self.upload.is_none() {
            self.upload = Some(self.shared.borrow_mut().renderer.begin_game_atlas()?);
        }
        self.upload
            .as_mut()
            .ok_or("No atlas upload")?
            .page_buffer(index)
    }
    fn commit(&mut self, index: u32) -> Result<(), String> {
        let upload = self.upload.as_mut().ok_or("No atlas upload")?;
        self.shared
            .borrow_mut()
            .renderer
            .write_game_atlas_page(upload, index)
    }
    fn finish(&mut self) -> Result<(), String> {
        let upload = self.upload.take().ok_or("No atlas pages written")?;
        self.shared.borrow_mut().renderer.finish_game_atlas(upload)
    }
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
                // The client is borrowed only while a page is handed over, never
                // across a decode await, so rendering continues on the old atlas.
                match load(&mut SharedSink::new(&shared)).await {
                    Ok((art, colour)) => {
                        let mut client = shared.borrow_mut();
                        if let Some(element) = client.document.get_element_by_id("playground") {
                            let _ =
                                element.set_attribute("data-art-colour", &format!("{colour:?}"));
                        }
                        client.art = art;
                        client.team_colour = colour;
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

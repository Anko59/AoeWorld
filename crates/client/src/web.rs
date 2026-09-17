use aoe_core::{EntityId, Region, Tick};
use aoe_protocol::{
    ClientMessage, EntityState, ServerMessage, VERSION, decode_server, encode_client,
};
use aoe_rendering::{Camera, Counters, Renderer};
use js_sys::Uint8Array;
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};
use wasm_bindgen::{JsCast, JsValue, closure::Closure, prelude::wasm_bindgen};
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::{
    Document, Event, HtmlCanvasElement, HtmlSelectElement, KeyboardEvent, MessageEvent, Request,
    RequestInit, Response, WebSocket, WheelEvent,
};

mod metrics;
use metrics::{Metrics, now_ms, sample};

struct Client {
    document: Document,
    canvas: HtmlCanvasElement,
    renderer: Renderer,
    adapter: String,
    socket: Option<WebSocket>,
    connection_id: u64,
    camera: Camera,
    region: Option<Region>,
    entities: BTreeMap<EntityId, EntityState>,
    tick: Tick,
    total_entities: u32,
    loaded_chunks: u32,
    build: String,
    scenario: String,
    world_size: i32,
    status: String,
    paused: bool,
    counters: Counters,
    metrics: Metrics,
    first_visible_ms: Option<f64>,
}

fn set_text(document: &Document, id: &str, value: &str) {
    if let Some(node) = document.get_element_by_id(id) {
        node.set_text_content(Some(value));
    }
}

fn visible_region(client: &Client) -> Region {
    let width = ((client.canvas.width() as f32 / client.camera.zoom).ceil() as u16).clamp(1, 512);
    let height = ((client.canvas.height() as f32 / client.camera.zoom).ceil() as u16).clamp(1, 512);
    let max_x = client.world_size - i32::from(width);
    let max_y = client.world_size - i32::from(height);
    Region {
        x: (client.camera.x as i32).clamp(0, max_x),
        y: (client.camera.y as i32).clamp(0, max_y),
        width,
        height,
    }
}

fn subscribe(client: &mut Client) {
    let region = visible_region(client);
    if client.region == Some(region) {
        return;
    }
    if client.status != "connected" {
        return;
    }
    if let Some(socket) = &client.socket {
        if socket.ready_state() == WebSocket::OPEN {
            if let Ok(bytes) = encode_client(&ClientMessage::Subscribe { region }) {
                match socket.send_with_u8_array(&bytes) {
                    Ok(()) => client.region = Some(region),
                    Err(_) => client.status = "subscription send failed".to_owned(),
                }
            }
        }
    }
}

fn connect(shared: Rc<RefCell<Client>>) -> Result<(), JsValue> {
    let location = shared
        .borrow()
        .document
        .location()
        .ok_or("No document location")?;
    let protocol = if location.protocol()? == "https:" {
        "wss"
    } else {
        "ws"
    };
    let socket = WebSocket::new(&format!("{protocol}://{}/ws", location.host()?))?;
    socket.set_binary_type(web_sys::BinaryType::Arraybuffer);
    let connection_id = {
        let mut client = shared.borrow_mut();
        client.connection_id += 1;
        if let Some(old) = client.socket.replace(socket.clone()) {
            let _ = old.close();
        }
        client.region = None;
        client.status = "connecting".to_owned();
        client.connection_id
    };
    let opened = shared.clone();
    let onopen = Closure::<dyn FnMut(Event)>::new(move |_| {
        let mut client = opened.borrow_mut();
        if client.connection_id != connection_id {
            return;
        }
        client.status = "negotiating".to_owned();
        if let Some(socket) = &client.socket {
            if let Ok(bytes) = encode_client(&ClientMessage::Hello { version: VERSION }) {
                let _ = socket.send_with_u8_array(&bytes);
            }
        }
    });
    socket.set_onopen(Some(onopen.as_ref().unchecked_ref()));
    onopen.forget();

    let received = shared.clone();
    let onmessage = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
        let started = now_ms();
        let bytes = Uint8Array::new(&event.data()).to_vec();
        let mut client = received.borrow_mut();
        if client.connection_id != connection_id {
            return;
        }
        match decode_server(&bytes) {
            Ok(ServerMessage::Hello {
                version: VERSION,
                build,
                scenario,
                world_size,
            }) => {
                client.build = build;
                client.scenario = scenario;
                client.world_size = world_size;
                client.region = None;
                client.entities.clear();
                client.total_entities = 0;
                if let Some(select) = client
                    .document
                    .get_element_by_id("scenario")
                    .and_then(|element| element.dyn_into::<HtmlSelectElement>().ok())
                {
                    select.set_value(&client.scenario);
                }
                client.status = "connected".to_owned();
                subscribe(&mut client);
            }
            Ok(ServerMessage::Snapshot {
                tick,
                entities,
                loaded_chunks,
                total_entities,
                ..
            }) => {
                client.tick = tick;
                client.entities = entities.into_iter().map(|e| (e.id, e)).collect();
                client.loaded_chunks = loaded_chunks;
                client.total_entities = total_entities;
            }
            Ok(ServerMessage::Delta {
                tick,
                upserts,
                removals,
            }) => {
                client.tick = tick;
                for id in removals {
                    client.entities.remove(&id);
                }
                for entity in upserts {
                    client.entities.insert(entity.id, entity);
                }
            }
            Ok(ServerMessage::Error { message, .. }) => {
                client.status = format!("server error: {message}")
            }
            _ => client.status = "protocol error".to_owned(),
        }
        sample(&mut client.metrics.decode_update_ms, now_ms() - started);
        client.metrics.messages += 1;
        client.metrics.maximum_resident =
            client.metrics.maximum_resident.max(client.entities.len());
    });
    socket.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
    onmessage.forget();

    let closed = shared.clone();
    let onclose = Closure::<dyn FnMut(Event)>::new(move |_| {
        let mut client = closed.borrow_mut();
        if client.connection_id != connection_id {
            return;
        }
        client.status = "disconnected; reconnect available".to_owned();
        client.socket = None;
    });
    socket.set_onclose(Some(onclose.as_ref().unchecked_ref()));
    onclose.forget();
    Ok(())
}

fn install_controls(shared: Rc<RefCell<Client>>) -> Result<(), JsValue> {
    let document = shared.borrow().document.clone();
    let moved = shared.clone();
    let onkey = Closure::<dyn FnMut(KeyboardEvent)>::new(move |event: KeyboardEvent| {
        if event
            .target()
            .is_some_and(|target| target.dyn_into::<HtmlSelectElement>().is_ok())
        {
            return;
        }
        let mut client = moved.borrow_mut();
        let step = 32.0;
        match event.key().as_str() {
            "ArrowLeft" | "a" => client.camera.x -= step,
            "ArrowRight" | "d" => client.camera.x += step,
            "ArrowUp" | "w" => client.camera.y -= step,
            "ArrowDown" | "s" => client.camera.y += step,
            "+" | "=" => client.camera.zoom = (client.camera.zoom * 1.25).min(16.0),
            "-" => client.camera.zoom = (client.camera.zoom / 1.25).max(2.5),
            " " => client.paused = !client.paused,
            _ => return,
        }
        event.prevent_default();
        client.camera.x = client.camera.x.max(0.0);
        client.camera.y = client.camera.y.max(0.0);
        subscribe(&mut client);
    });
    document.add_event_listener_with_callback("keydown", onkey.as_ref().unchecked_ref())?;
    onkey.forget();

    let zoomed = shared.clone();
    let onwheel = Closure::<dyn FnMut(WheelEvent)>::new(move |event: WheelEvent| {
        event.prevent_default();
        let mut client = zoomed.borrow_mut();
        let factor = if event.delta_y() < 0.0 {
            1.1
        } else {
            1.0 / 1.1
        };
        client.camera.zoom = (client.camera.zoom * factor).clamp(2.5, 16.0);
        subscribe(&mut client);
    });
    document.add_event_listener_with_callback("wheel", onwheel.as_ref().unchecked_ref())?;
    onwheel.forget();

    if let Some(button) = document.get_element_by_id("reconnect") {
        let reconnect = shared.clone();
        let onclick = Closure::<dyn FnMut(Event)>::new(move |_| {
            let _ = connect(reconnect.clone());
        });
        button.add_event_listener_with_callback("click", onclick.as_ref().unchecked_ref())?;
        onclick.forget();
    }
    if let Some(select) = document.get_element_by_id("scenario") {
        let select: HtmlSelectElement = select.dyn_into()?;
        let chosen = select.clone();
        let changed = shared.clone();
        let onchange = Closure::<dyn FnMut(Event)>::new(move |_| {
            let name = chosen.value();
            let changed = changed.clone();
            spawn_local(async move {
                if let Err(error) = change_scenario(&name).await {
                    changed.borrow_mut().status = format!("scenario change failed: {error:?}");
                }
            });
        });
        select.add_event_listener_with_callback("change", onchange.as_ref().unchecked_ref())?;
        onchange.forget();
    }
    Ok(())
}

async fn change_scenario(name: &str) -> Result<(), JsValue> {
    let window = web_sys::window().ok_or("No window")?;
    let options = RequestInit::new();
    options.set_method("POST");
    let request = Request::new_with_str_and_init(&format!("/scenario/{name}"), &options)?;
    let response: Response = JsFuture::from(window.fetch_with_request(&request))
        .await?
        .dyn_into()?;
    if !response.ok() {
        return Err(JsValue::from_str(&format!("HTTP {}", response.status())));
    }
    Ok(())
}

fn animate(shared: Rc<RefCell<Client>>) -> Result<(), JsValue> {
    let callback = Rc::new(RefCell::new(None::<Closure<dyn FnMut(f64)>>));
    let next = callback.clone();
    *callback.borrow_mut() = Some(Closure::new(move |time: f64| {
        let started = now_ms();
        let mut client = shared.borrow_mut();
        if !client.paused {
            if let Some(previous) = client.metrics.last_frame_time {
                sample(&mut client.metrics.frame_intervals_ms, time - previous);
            }
            client.metrics.last_frame_time = Some(time);
            let width = client.canvas.client_width().max(1) as u32;
            let height = client.canvas.client_height().max(1) as u32;
            if client.canvas.width() != width {
                client.canvas.set_width(width);
            }
            if client.canvas.height() != height {
                client.canvas.set_height(height);
            }
            client.renderer.resize(width, height);
            subscribe(&mut client);
            let entities: Vec<_> = client.entities.values().copied().collect();
            let camera = client.camera;
            match client.renderer.render(entities.into_iter(), camera) {
                Ok(counters) => client.counters = counters,
                Err(error) => client.status = error,
            }
            client.metrics.frames += 1;
            if client.counters.visible > 0 {
                client.first_visible_ms.get_or_insert_with(now_ms);
            }
            client.metrics.maximum_visible =
                client.metrics.maximum_visible.max(client.counters.visible);
            sample(&mut client.metrics.cpu_submission_ms, now_ms() - started);
        } else {
            client.metrics.last_frame_time = None;
        }
        let diagnostics = format!(
            "build: {} | scenario: {} | adapter: {} | connection: {} | tick: {} | camera: {},{} | zoom: {:.2} | resident: {} / {} | visible: {} | chunks: {} | draws: {} | GPU buffer: {} B | {}{}",
            client.build,
            client.scenario,
            client.adapter,
            client.connection_id,
            client.tick.0,
            client.camera.x as i32,
            client.camera.y as i32,
            client.camera.zoom,
            client.entities.len(),
            client.total_entities,
            client.counters.visible,
            client.loaded_chunks,
            client.counters.draw_calls,
            client.counters.gpu_buffer_bytes,
            client.status,
            if client.paused { " | view paused" } else { "" }
        );
        set_text(&client.document, "diagnostics", &diagnostics);
        drop(client);
        if let Some(window) = web_sys::window() {
            if let Some(cb) = next.borrow().as_ref() {
                let _ = window.request_animation_frame(cb.as_ref().unchecked_ref());
            }
        }
    }));
    let window = web_sys::window().ok_or("No window")?;
    if let Some(cb) = callback.borrow().as_ref() {
        window.request_animation_frame(cb.as_ref().unchecked_ref())?;
    }
    Ok(())
}

async fn initialize() -> Result<(), JsValue> {
    let window = web_sys::window().ok_or("No window")?;
    let document = window.document().ok_or("No document")?;
    let canvas: HtmlCanvasElement = document
        .get_element_by_id("scene")
        .ok_or("No scene canvas")?
        .dyn_into()?;
    let renderer = Renderer::new(canvas.clone())
        .await
        .map_err(|error| JsValue::from_str(&error))?;
    let adapter = renderer.adapter_label().to_owned();
    let shared = Rc::new(RefCell::new(Client {
        document,
        canvas,
        renderer,
        adapter,
        socket: None,
        connection_id: 0,
        camera: Camera {
            x: 0.0,
            y: 0.0,
            zoom: 4.0,
        },
        region: None,
        entities: BTreeMap::new(),
        tick: Tick(0),
        total_entities: 0,
        loaded_chunks: 0,
        build: "pending".to_owned(),
        scenario: "pending".to_owned(),
        world_size: 1_024,
        status: "starting".to_owned(),
        paused: false,
        counters: Counters::default(),
        metrics: Metrics::default(),
        first_visible_ms: None,
    }));
    metrics::set_active(&shared);
    install_controls(shared.clone())?;
    connect(shared.clone())?;
    animate(shared)?;
    Ok(())
}

#[wasm_bindgen(start)]
pub fn start() {
    spawn_local(async {
        if let Err(error) = initialize().await {
            if let Some(document) = web_sys::window().and_then(|window| window.document()) {
                set_text(
                    &document,
                    "unsupported",
                    &format!(
                        "WebGPU unavailable: {error:?}. Try a WebGPU-capable desktop browser and reload."
                    ),
                );
            }
        }
    });
}

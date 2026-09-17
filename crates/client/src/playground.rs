//! Browser input and fixed-step scheduling for the local playground.
use aoe_core::Position;
use aoe_rendering::GameRenderer;
use aoe_simulation::playground::{HEIGHT, Playground, WIDTH};
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use web_sys::{Document, Event, HtmlCanvasElement, KeyboardEvent, PointerEvent};

pub async fn initialize(document: Document) -> Result<(), JsValue> {
    let canvas: HtmlCanvasElement = document
        .get_element_by_id("scene")
        .ok_or("No canvas")?
        .dyn_into()?;
    let (mut renderer, canvas) = GameRenderer::new(canvas)
        .await
        .map_err(|e| JsValue::from_str(&e))?;
    document
        .get_element_by_id("playground")
        .ok_or("No game")?
        .set_attribute("data-renderer", renderer.backend())?;
    let (art, pixels) = crate::game_assets::load().await?;
    renderer
        .upload_game_atlas(&pixels)
        .map_err(|e| JsValue::from_str(&e))?;
    drop(pixels);
    document
        .get_element_by_id("playground")
        .ok_or("No game")?
        .set_attribute("data-assets", "aoe2-local")?;
    let world = Rc::new(RefCell::new(Playground::default()));
    let clicked = world.clone();
    let surface = canvas.clone();
    let pointer = Closure::<dyn FnMut(PointerEvent)>::new(move |event: PointerEvent| {
        if event.button() != 0 && event.button() != 2 {
            return;
        }
        event.prevent_default();
        let _ = surface.focus();
        let bounds = surface.get_bounding_client_rect();
        if bounds.width() <= 0.0 || bounds.height() <= 0.0 {
            return;
        }
        clicked.borrow_mut().move_to(Position {
            x: ((f64::from(event.client_x()) - bounds.left()) / bounds.width() * f64::from(WIDTH))
                as i32,
            y: ((f64::from(event.client_y()) - bounds.top()) / bounds.height() * f64::from(HEIGHT))
                as i32,
        });
    });
    canvas.add_event_listener_with_callback("pointerdown", pointer.as_ref().unchecked_ref())?;
    pointer.forget();
    let context = Closure::<dyn FnMut(Event)>::new(move |event: Event| event.prevent_default());
    canvas.add_event_listener_with_callback("contextmenu", context.as_ref().unchecked_ref())?;
    context.forget();
    let reset = world.clone();
    let button = document
        .get_element_by_id("reset")
        .ok_or("No reset button")?;
    button.remove_attribute("disabled")?;
    let onreset =
        Closure::<dyn FnMut(Event)>::new(move |_| *reset.borrow_mut() = Playground::default());
    button.add_event_listener_with_callback("click", onreset.as_ref().unchecked_ref())?;
    onreset.forget();
    let keyed = world.clone();
    let onkey = Closure::<dyn FnMut(KeyboardEvent)>::new(move |event: KeyboardEvent| {
        let (dx, dy) = match event.key().as_str() {
            "ArrowLeft" | "a" => (-60, 0),
            "ArrowRight" | "d" => (60, 0),
            "ArrowUp" | "w" => (0, -60),
            "ArrowDown" | "s" => (0, 60),
            _ => return,
        };
        event.prevent_default();
        let mut world = keyed.borrow_mut();
        let p = world.position;
        world.move_to(Position {
            x: p.x + dx,
            y: p.y + dy,
        });
    });
    canvas.add_event_listener_with_callback("keydown", onkey.as_ref().unchecked_ref())?;
    onkey.forget();

    let status = document
        .get_element_by_id("unit-status")
        .ok_or("No status")?;
    let position = document
        .get_element_by_id("position")
        .ok_or("No position")?;
    let alert = document
        .get_element_by_id("unsupported")
        .ok_or("No alert")?;
    let callback = Rc::new(RefCell::new(None::<Closure<dyn FnMut(f64)>>));
    let next = callback.clone();
    let mut previous: Option<f64> = None;
    let mut accumulator = 0.0;
    let mut facing = (0, false);
    let mut animation = 0;
    *callback.borrow_mut() = Some(Closure::new(move |time: f64| {
        if let Some(last) = previous {
            accumulator += (time - last).clamp(0.0, 100.0);
        }
        previous = Some(time);
        let mut world = world.borrow_mut();
        while accumulator >= 20.0 {
            world.advance();
            animation += 1;
            accumulator -= 20.0;
        }
        let width = canvas.client_width().max(1) as u32;
        let height = canvas.client_height().max(1) as u32;
        if canvas.width() != width || canvas.height() != height {
            canvas.set_width(width);
            canvas.set_height(height);
            renderer.resize(width, height);
        }
        let p = world.position;
        let t = world.destination;
        if world.moving() {
            let dx = t.x - p.x;
            let dy = t.y - p.y;
            let angle = (dx.abs() as f32).atan2(dy as f32);
            facing = (
                (angle / std::f32::consts::FRAC_PI_4).round() as usize,
                dx > 0,
            );
        }
        if let Err(error) = renderer.render_game(
            &art,
            [p.x as f32, p.y as f32],
            [t.x as f32, t.y as f32],
            world.moving(),
            animation / 4,
            facing,
        ) {
            status.set_text_content(Some("Unavailable"));
            alert.set_text_content(Some(&error));
            return;
        }
        status.set_text_content(Some(if world.moving() { "Moving" } else { "Ready" }));
        position.set_text_content(Some(&format!("{}, {}", p.x, p.y)));
        if let Some(window) = web_sys::window() {
            if let Some(cb) = next.borrow().as_ref() {
                let _ = window.request_animation_frame(cb.as_ref().unchecked_ref());
            }
        }
    }));
    if let Some(cb) = callback.borrow().as_ref() {
        web_sys::window()
            .ok_or("No window")?
            .request_animation_frame(cb.as_ref().unchecked_ref())?;
    }
    Ok(())
}

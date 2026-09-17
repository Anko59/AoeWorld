use super::Client;
use js_sys::{Array, Object, Reflect};
use std::{
    cell::RefCell,
    collections::VecDeque,
    rc::{Rc, Weak},
};
use wasm_bindgen::{JsValue, prelude::wasm_bindgen};

const MAX_TIMING_SAMPLES: usize = 3_600;

thread_local! {
    static ACTIVE_CLIENT: RefCell<Option<Weak<RefCell<Client>>>> = const { RefCell::new(None) };
}

#[derive(Default)]
pub(super) struct Metrics {
    pub(super) last_frame_time: Option<f64>,
    pub(super) frame_intervals_ms: VecDeque<f64>,
    pub(super) cpu_submission_ms: VecDeque<f64>,
    pub(super) decode_update_ms: VecDeque<f64>,
    pub(super) frames: u64,
    pub(super) messages: u64,
    pub(super) maximum_visible: usize,
    pub(super) maximum_resident: usize,
}

pub(super) fn sample(samples: &mut VecDeque<f64>, value: f64) {
    if samples.len() == MAX_TIMING_SAMPLES {
        samples.pop_front();
    }
    samples.push_back(value);
}

pub(super) fn now_ms() -> f64 {
    web_sys::window()
        .and_then(|window| window.performance())
        .map_or(0.0, |performance| performance.now())
}

fn wasm_memory_bytes() -> Option<u64> {
    let memory = wasm_bindgen::memory();
    let buffer = js_sys::Reflect::get(&memory, &JsValue::from_str("buffer")).ok()?;
    js_sys::Reflect::get(&buffer, &JsValue::from_str("byteLength"))
        .ok()?
        .as_f64()
        .map(|bytes| bytes as u64)
}

fn set(object: &Object, name: &str, value: JsValue) -> Result<(), JsValue> {
    Reflect::set(object, &JsValue::from_str(name), &value).map(|_| ())
}

fn number(value: usize) -> JsValue {
    JsValue::from_f64(value as f64)
}

fn optional_number(value: Option<f64>) -> JsValue {
    value.map_or(JsValue::NULL, JsValue::from_f64)
}

fn samples(values: &VecDeque<f64>) -> Array {
    let result = Array::new();
    for value in values {
        result.push(&JsValue::from_f64(*value));
    }
    result
}

#[wasm_bindgen]
pub fn performance_snapshot() -> Result<JsValue, JsValue> {
    ACTIVE_CLIENT.with(|active| {
        let client = active
            .borrow()
            .as_ref()
            .and_then(Weak::upgrade)
            .ok_or_else(|| JsValue::from_str("browser client is not initialized"))?;
        let client = client.borrow();
        let report = Object::new();
        set(&report, "version", JsValue::from_f64(1.0))?;
        set(&report, "build", JsValue::from_str(&client.build))?;
        set(&report, "scenario", JsValue::from_str(&client.scenario))?;
        set(
            &report,
            "connection_id",
            JsValue::from_f64(client.connection_id as f64),
        )?;
        set(
            &report,
            "viewport_width",
            JsValue::from_f64(client.canvas.width() as f64),
        )?;
        set(
            &report,
            "viewport_height",
            JsValue::from_f64(client.canvas.height() as f64),
        )?;
        set(
            &report,
            "total_entities",
            JsValue::from_f64(client.total_entities as f64),
        )?;
        set(&report, "resident_entities", number(client.entities.len()))?;
        set(&report, "visible_entities", number(client.counters.visible))?;
        set(
            &report,
            "loaded_chunks",
            JsValue::from_f64(client.loaded_chunks as f64),
        )?;
        set(
            &report,
            "frames",
            JsValue::from_f64(client.metrics.frames as f64),
        )?;
        set(
            &report,
            "messages",
            JsValue::from_f64(client.metrics.messages as f64),
        )?;
        set(
            &report,
            "maximum_visible",
            number(client.metrics.maximum_visible),
        )?;
        set(
            &report,
            "maximum_resident",
            number(client.metrics.maximum_resident),
        )?;
        set(
            &report,
            "frame_intervals_ms",
            samples(&client.metrics.frame_intervals_ms).into(),
        )?;
        set(
            &report,
            "cpu_submission_ms",
            samples(&client.metrics.cpu_submission_ms).into(),
        )?;
        set(
            &report,
            "decode_update_ms",
            samples(&client.metrics.decode_update_ms).into(),
        )?;
        set(
            &report,
            "wasm_memory_bytes",
            wasm_memory_bytes().map_or(JsValue::NULL, |value| JsValue::from_f64(value as f64)),
        )?;
        set(
            &report,
            "first_visible_ms",
            optional_number(client.first_visible_ms),
        )?;
        set(
            &report,
            "gpu_buffer_bytes",
            number(client.counters.gpu_buffer_bytes),
        )?;
        set(
            &report,
            "persistent_gpu_resources",
            number(client.counters.persistent_gpu_resources),
        )?;
        set(&report, "atlas_pages", number(client.counters.atlas_pages))?;
        set(
            &report,
            "atlas_uploads",
            number(client.counters.atlas_uploads),
        )?;
        set(&report, "atlas_bytes", number(client.counters.atlas_bytes))?;
        Ok(report.into())
    })
}

#[wasm_bindgen]
pub fn reset_performance_samples() -> Result<(), JsValue> {
    ACTIVE_CLIENT.with(|active| {
        let client = active
            .borrow()
            .as_ref()
            .and_then(Weak::upgrade)
            .ok_or_else(|| JsValue::from_str("browser client is not initialized"))?;
        client.borrow_mut().metrics = Metrics::default();
        Ok(())
    })
}

pub(super) fn set_active(client: &Rc<RefCell<Client>>) {
    ACTIVE_CLIENT.with(|active| *active.borrow_mut() = Some(Rc::downgrade(client)));
}

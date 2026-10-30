//! Bound outstanding browser work, including queues which never settle.
use std::{
    future::{Future, poll_fn},
    pin::pin,
    sync::{Arc, Mutex},
    task::Poll,
};
use wasm_bindgen_futures::JsFuture;

#[path = "health/tests.rs"]
#[cfg(test)]
mod tests;

const DEADLINE_MS: f64 = 5_000.0;
const MAX_FRAMES: usize = 2;
const MAX_UPLOADS: usize = crate::MAX_GAME_ATLAS_PAGES as usize;

#[derive(Default)]
struct State {
    error: Option<String>,
    waiting_since: Option<f64>,
    frames: usize,
    uploads: usize,
}

#[derive(Clone, Default)]
pub(crate) struct Health(Arc<Mutex<State>>);

fn now() -> f64 {
    web_sys::window()
        .and_then(|window| window.performance())
        .map_or(0.0, |clock| clock.now())
}

impl Health {
    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(super) fn install(&self, device: &wgpu::Device) {
        let health = self.clone();
        device.on_uncaptured_error(Arc::new(move |error| {
            health.fail(format!("WebGPU uncaptured error: {error}"));
        }));
        let health = self.clone();
        device.set_device_lost_callback(move |reason, message| {
            health.fail(format!("WebGPU device lost ({reason:?}): {message}"));
        });
    }

    fn fail(&self, message: String) {
        // Log every event, retain only the first reason. Never retain frames,
        // upload bytes, devices or browser handles in these callbacks.
        web_sys::console::error_1(&message.clone().into());
        self.state().error.get_or_insert(message);
    }

    pub(super) fn check(&self) -> Result<(), String> {
        let state = self.state();
        if let Some(error) = &state.error {
            return Err(error.clone());
        }
        if state
            .waiting_since
            .is_some_and(|at| now() - at >= DEADLINE_MS)
        {
            return Err("WebGPU produced no completed frame/upload within 5 seconds".into());
        }
        Ok(())
    }

    pub(super) fn ready(&self) -> Result<bool, String> {
        self.check()?;
        let mut state = self.state();
        state.waiting_since.get_or_insert_with(now);
        Ok(state.frames < MAX_FRAMES)
    }
    pub(super) fn uploads_ready(&self) -> Result<bool, String> {
        self.check()?;
        Ok(self.state().frames < MAX_FRAMES)
    }

    pub(crate) fn allow_upload(&self) -> Result<(), String> {
        self.check()?;
        if self.state().uploads >= MAX_UPLOADS {
            return Err("WebGPU pending atlas uploads exceeded one atlas".into());
        }
        Ok(())
    }

    pub(crate) fn submitted(&self, queue: &wgpu::Queue, upload: bool) {
        {
            let mut state = self.state();
            state.waiting_since.get_or_insert_with(now);
            if upload {
                state.uploads += 1;
            } else {
                state.frames += 1;
            }
        }
        let health = self.clone();
        // wgpu's callback treats a rejected browser promise as completed work.
        // Observe the real promise so rejection also triggers recovery/logging.
        let Some(queue) = queue.as_webgpu() else {
            self.fail("WebGPU queue has no browser completion promise".into());
            return;
        };
        let promise = queue.on_submitted_work_done();
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(error) = JsFuture::from(promise).await {
                health.fail(format!("WebGPU queue completion rejected: {error:?}"));
            }
            health.completed(upload);
        });
    }

    fn completed(&self, upload: bool) {
        let mut state = self.state();
        if upload {
            state.uploads -= 1;
        } else {
            state.frames -= 1;
        }
        // A continuously busy queue can make progress without becoming empty.
        // Bound the time since its last completion, not its first submission.
        state.waiting_since = if state.frames == 0 && state.uploads == 0 {
            None
        } else {
            Some(now())
        };
    }
}

/// Adapter/device requests can hang without ever returning an error. Drop the
/// abandoned future, clear its timer and let the caller select the next tier.
pub(crate) async fn within<T>(
    future: impl Future<Output = Result<T, String>>,
) -> Result<T, String> {
    let window = web_sys::window().ok_or("No window for WebGPU deadline")?;
    let mut timer = Err(wasm_bindgen::JsValue::UNDEFINED);
    let deadline = js_sys::Promise::new(&mut |resolve, _| {
        timer = window
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, DEADLINE_MS as i32);
    });
    let timer = timer.map_err(|error| format!("WebGPU deadline: {error:?}"))?;
    let mut deadline = pin!(JsFuture::from(deadline));
    let mut future = pin!(future);
    let result = poll_fn(|context| {
        if let Poll::Ready(value) = future.as_mut().poll(context) {
            return Poll::Ready(value);
        }
        if deadline.as_mut().poll(context).is_ready() {
            return Poll::Ready(Err("WebGPU initialization timed out after 5 seconds".into()));
        }
        Poll::Pending
    })
    .await;
    window.clear_timeout_with_handle(timer);
    result
}

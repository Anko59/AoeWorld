//! A client that gives up while its file is being encoded does not waste the
//! encoder's run: the finished bytes are kept, and the next request gets them
//! without another run.
use super::*;
use axum::{
    extract::Request,
    http::StatusCode,
    middleware::{Next, from_fn},
    response::{IntoResponse, Response},
};
use std::{
    sync::{Arc, mpsc},
    task::{Context, Poll, Wake, Waker},
};
use tokio::sync::Notify;

/// Tells the stepper that the answer it polls can go on.
struct Woken(Notify);

impl Wake for Woken {
    fn wake(self: Arc<Self>) {
        self.0.notify_one();
    }
}

/// Stands where the connection does and polls the application's answer itself.
/// Asked with `abandon-at: n`, it drops the answer the n-th time it is not
/// ready, as a closed connection does. Otherwise the answer leaves with the
/// number of times it was not ready, in `waits`.
///
/// The runtime of this test has one blocking thread, and a gate holds it during
/// each poll. Whatever blocking work the answer starts there (a file call, the
/// encoder) is therefore still queued when the poll returns: every wait is one
/// such piece of work, whatever the machine's timing.
async fn stepper(request: Request, next: Next) -> Response {
    let abandon_at = request.headers().get("abandon-at").cloned();
    let woken = Arc::new(Woken(Notify::new()));
    let waker = Waker::from(Arc::clone(&woken));
    let mut answer = Box::pin(next.run(request));
    let mut waits = 0_u32;
    loop {
        let (release, held) = mpsc::channel::<()>();
        tokio::task::spawn_blocking(move || held.recv());
        let polled = answer.as_mut().poll(&mut Context::from_waker(&waker));
        drop(release);
        match polled {
            Poll::Ready(mut response) => {
                response.headers_mut().insert("waits", waits.into());
                return response;
            }
            Poll::Pending => waits += 1,
        }
        if abandon_at == Some(waits.into()) {
            drop(answer);
            return StatusCode::NO_CONTENT.into_response();
        }
        woken.0.notified().await;
    }
}

/// The count: a first request dropped while it waits for the encoder leaves
/// one run of the encoder and its bytes behind, and the next request for that
/// file is served from them.
#[test]
fn static_abandoned_request_leaves_its_encoding_for_the_next() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(1)
        .build()
        .unwrap();
    runtime.block_on(async {
        // The same application once more, behind the stepper.
        let (_, state) = serve().await;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let router = app(state.clone()).layer(from_fn(stepper));
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });

        let (url, twin) = ("/pkg/abandoned.js", "/pkg/twin.js");
        let file = text(1, 8 * KIB);
        write_at(&on_disk(url), &file, MODIFIED);
        write_at(&on_disk(twin), &file, MODIFIED);
        let plain = request(address, "GET", url, &[]).await;

        // The first request for the twin, left to finish, shows where a first
        // request waits: the last time is for the encoder.
        let whole = request(address, "GET", twin, &[GZIP]).await;
        whole.assert_gzip(&file, &plain);
        let waits = whole.all("waits")[0];
        assert_eq!(compressions(&state), 1);

        let abandon = format!("abandon-at: {waits}");
        let abandoned = request(address, "GET", url, &[GZIP, abandon.as_str()]).await;
        assert_eq!(abandoned.status, 204, "{}", abandoned.seen);
        // The encoder was started. It is given a bounded time to finish.
        for _ in 0..500 {
            if compressions(&state) > 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let kept = compressions(&state);
        assert_eq!(kept, 2, "encoding thrown away, abandoned at wait {waits}");

        let next = request(address, "GET", url, &[GZIP]).await;
        next.assert_gzip(&file, &plain);
        assert_eq!(compressions(&state), 2, "the next request encoded again");
    });
}

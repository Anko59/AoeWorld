//! A map is ready once its minimap raster has been served: the server answers
//! 503 when computing it outruns its budget, and the client never retries.
use crate::devserver::{
    process::http_status,
    warm::{completed, maps, raster},
};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};

/// A server whose n-th reply has the n-th status (the last repeats); a 200
/// carries a body larger than one read.
struct Scripted {
    port: u16,
    requests: Arc<AtomicUsize>,
}

impl Scripted {
    fn answering(statuses: &[u16], jobs: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("address").port();
        let requests = Arc::new(AtomicUsize::new(0));
        let (seen, statuses, jobs) = (requests.clone(), statuses.to_vec(), jobs.to_owned());
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut buffer = [0_u8; 512];
                let count = stream.read(&mut buffer).unwrap_or(0);
                let request = String::from_utf8_lossy(&buffer[..count]).into_owned();
                if request.starts_with("GET /maps/jobs") {
                    let _ = write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{jobs}",
                        jobs.len()
                    );
                    continue;
                }
                let n = seen.fetch_add(1, Ordering::SeqCst);
                let status = statuses[n.min(statuses.len() - 1)];
                let body = if status == 200 {
                    vec![7_u8; 200_000]
                } else {
                    Vec::new()
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(&body);
            }
        });
        Self { port, requests }
    }

    fn rasters_asked(&self) -> usize {
        self.requests.load(Ordering::SeqCst)
    }
}

#[test]
fn the_status_is_read_from_a_reply_larger_than_one_read() {
    let server = Scripted::answering(&[200], "[]");
    let status = http_status(server.port, "/maps/h/minimap", Duration::from_secs(5));
    assert_eq!(status, Some(200));
}

#[test]
fn a_raster_that_ran_out_of_budget_is_asked_for_again() {
    let server = Scripted::answering(&[503, 503, 200], "[]");
    let took = raster(server.port, "abc", Duration::from_secs(30)).expect("served");
    assert_eq!(server.rasters_asked(), 3);
    assert!(took >= Duration::from_secs(4), "two retries wait: {took:?}");
}

#[test]
fn a_map_the_server_does_not_have_fails_at_once_naming_the_status() {
    let server = Scripted::answering(&[404], "[]");
    let error = raster(server.port, "abc", Duration::from_secs(30)).unwrap_err();
    assert!(error.contains("404") && error.contains("abc"), "{error}");
    assert_eq!(server.rasters_asked(), 1);
}

#[test]
fn a_raster_that_never_comes_fails_within_its_budget() {
    let server = Scripted::answering(&[503], "[]");
    let error = raster(server.port, "abc", Duration::from_millis(1500)).unwrap_err();
    assert!(error.contains("not served within"), "{error}");
    assert!(error.contains("503"), "{error}");
}

fn want() -> Value {
    json!({"center_latitude_e7": 1, "center_longitude_e7": 2, "requested_side_meters": 3,
        "seed": 1})
}

#[test]
fn only_completed_jobs_of_the_wanted_map_name_a_raster_once_each() {
    let job = |state: &str, hash: &str, side: u64| {
        json!({"state": state, "content_hash": hash, "request": {
            "center_latitude_e7": 1, "center_longitude_e7": 2, "requested_side_meters": side}})
    };
    let jobs = json!([
        job("completed", "aa", 3),
        job("completed", "aa", 3),
        job("running", "bb", 3),
        job("completed", "cc", 4),
        job("completed", "dd", 3),
    ])
    .to_string();
    assert_eq!(completed(&jobs, &want()), ["aa", "dd"]);
    assert!(completed("not json", &want()).is_empty());
}

#[test]
fn maps_warms_each_completed_map_of_the_wanted_list() {
    let jobs = json!([{"state": "completed", "content_hash": "aa", "request": want()},
        {"state": "queued", "content_hash": null, "request": want()}])
    .to_string();
    let server = Scripted::answering(&[200], &jobs);
    let warmed = maps(server.port, &[want()], Duration::from_secs(5)).expect("warmed");
    assert_eq!(warmed.len(), 1);
    assert_eq!(warmed[0].0, "aa");
    assert_eq!(server.rasters_asked(), 1);
    let none = maps(
        server.port,
        &[json!({"center_latitude_e7": 9})],
        Duration::from_secs(5),
    );
    assert_eq!(none.expect("nothing to warm").len(), 0);
}

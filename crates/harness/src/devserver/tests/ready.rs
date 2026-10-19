//! The job list changes while a raster request is in flight.
use super::warm_presets_for;
use serde_json::json;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

fn warmed_before_return(budget: Duration, raster_delay: Duration) -> Vec<String> {
    let a = json!({"center_latitude_e7": 1, "center_longitude_e7": 2,
        "requested_side_meters": 30_000});
    let mut b = a.clone();
    b["center_latitude_e7"] = json!(3);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let rasters = Arc::new(Mutex::new(Vec::new()));
    let seen = rasters.clone();
    let wanted = [a.clone(), b.clone()];
    let server = thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let mut buffer = [0; 512];
            let count = stream.read(&mut buffer).unwrap();
            let request = String::from_utf8_lossy(&buffer[..count]);
            if request.starts_with("GET /stop ") {
                break;
            }
            let body = if request.starts_with("GET /maps/jobs ") {
                // B completes during A's request, before the next job read.
                let b_done = seen.lock().unwrap().contains(&"aa".to_owned());
                json!([
                    {"state": "completed", "content_hash": "aa", "request": a},
                    {"state": if b_done {"completed"} else {"running"},
                        "content_hash": if b_done {Some("bb")} else {None}, "request": b}
                ])
                .to_string()
            } else {
                let hash = request.split('/').nth(2).unwrap().to_owned();
                seen.lock().unwrap().push(hash);
                thread::sleep(raster_delay);
                String::new()
            };
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    warm_presets_for(port, &wanted, budget);
    let warmed = rasters.lock().unwrap().clone();
    crate::devserver::process::http_get(port, "/stop");
    server.join().unwrap();
    warmed
}

#[test]
fn a_preset_completed_during_another_raster_is_warmed_before_ready() {
    assert_eq!(
        warmed_before_return(Duration::from_secs(2), Duration::ZERO),
        ["aa", "bb"]
    );
}

#[test]
fn an_expired_overall_deadline_starts_no_more_raster_requests() {
    assert!(warmed_before_return(Duration::ZERO, Duration::ZERO).is_empty());
}

#[test]
fn a_slow_raster_cannot_restart_the_overall_budget_for_the_next_map() {
    assert_eq!(
        warmed_before_return(Duration::from_millis(500), Duration::from_secs(1)),
        ["aa"]
    );
}

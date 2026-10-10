use super::super::showcase::{Bridge, SOCKET};
use std::{
    io::{Read, Write},
    net::{Shutdown, TcpListener},
    os::unix::net::UnixStream,
    path::Path,
    thread,
};

fn work() -> tempfile::TempDir {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.cache/tmp");
    std::fs::create_dir_all(&root).unwrap();
    tempfile::Builder::new()
        .prefix("bridge-")
        .tempdir_in(root.canonicalize().unwrap())
        .unwrap()
}

/// A loopback app that echoes each connection until its peer stops writing.
fn echo_app() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            thread::spawn(move || {
                let _ = std::io::copy(&mut &stream, &mut &stream);
                let _ = stream.shutdown(Shutdown::Write);
            });
        }
    });
    port
}

fn round_trip(socket: &Path, message: &[u8]) -> Vec<u8> {
    let mut stream = UnixStream::connect(socket).unwrap();
    stream.write_all(message).unwrap();
    stream.shutdown(Shutdown::Write).unwrap();
    let mut reply = vec![];
    stream.read_to_end(&mut reply).unwrap();
    reply
}

fn echoes(stream: &mut UnixStream, message: &[u8]) -> bool {
    stream.write_all(message).unwrap();
    let mut reply = vec![0; message.len()];
    stream.read_exact(&mut reply).is_ok() && reply == message
}

fn refused(socket: &Path) -> bool {
    let mut stream = UnixStream::connect(socket).unwrap();
    let _ = stream.write_all(b"x");
    let mut reply = vec![];
    matches!(stream.read_to_end(&mut reply), Ok(0) | Err(_))
}

#[test]
fn the_bridge_forwards_its_socket_to_the_app_port_both_ways() {
    let dir = work();
    let socket = dir.path().join(SOCKET);
    let bridge = Bridge::start(dir.path(), "127.0.0.1", echo_app()).unwrap();
    assert_eq!(
        round_trip(&socket, b"GET / HTTP/1.1\r\n\r\n"),
        b"GET / HTTP/1.1\r\n\r\n"
    );
    assert_eq!(round_trip(&socket, b"again"), b"again");
    drop(bridge);
    // Stopped with the recording: the socket is gone.
    assert!(!socket.exists());
    assert!(UnixStream::connect(&socket).is_err());
}

#[test]
fn the_bridge_reaches_loopback_only() {
    let dir = work();
    for host in ["10.0.0.1", "192.0.2.1", "0.0.0.0"] {
        let error = Bridge::start(dir.path(), host, 80)
            .err()
            .unwrap()
            .to_string();
        assert!(error.contains("only to loopback"), "{error}");
        assert!(!dir.path().join(SOCKET).exists());
    }
}

#[test]
fn the_bridge_bounds_open_and_total_connections() {
    let dir = work();
    let socket = dir.path().join(SOCKET);
    let _bridge = Bridge::start_with(dir.path(), "127.0.0.1", echo_app(), 2, 3).unwrap();
    let mut first = UnixStream::connect(&socket).unwrap();
    let mut second = UnixStream::connect(&socket).unwrap();
    assert!(echoes(&mut first, b"one"));
    assert!(echoes(&mut second, b"two"));
    // Two are open: a third is closed unforwarded.
    assert!(refused(&socket));
    drop(first);
    drop(second);
    thread::sleep(std::time::Duration::from_millis(200));
    // One more fits the total of three; then none.
    assert_eq!(round_trip(&socket, b"three"), b"three");
    assert!(refused(&socket));
}

#[test]
fn stopping_the_bridge_closes_open_connections() {
    let dir = work();
    let socket = dir.path().join(SOCKET);
    let bridge = Bridge::start(dir.path(), "127.0.0.1", echo_app()).unwrap();
    let mut open = UnixStream::connect(&socket).unwrap();
    assert!(echoes(&mut open, b"idle keep-alive"));
    drop(bridge);
    let mut rest = vec![];
    assert!(matches!(open.read_to_end(&mut rest), Ok(0) | Err(_)));
}

#[test]
fn the_recorder_serves_the_app_port_from_the_bridge_socket_only() {
    let script = include_str!("../showcase/record.mjs");
    assert!(script.contains("import { createConnection, createServer } from \"node:net\";"));
    assert!(script.contains("createConnection(\"app.sock\")"));
    assert!(
        script.contains("server.listen(Number(new URL(app.http).port), \"127.0.0.1\", resolve);")
    );
    assert_eq!(script.matches("createConnection(").count(), 1);
    assert_eq!(script.matches("server.listen(").count(), 1);
    let bridge = script.find("await bridge(app)").unwrap();
    let launch = script.find("chromium.launch(").unwrap();
    let unbridge = script.find("unbridge();").unwrap();
    let closed = script.find("await browser.close();").unwrap();
    assert!(bridge < launch && closed < unbridge);
    // The in-Chromium origin filter stays as the second layer.
    assert!(script.contains("route.abort(\"blockedbyclient\")"));
}

#[test]
fn a_browser_scene_holds_its_seconds_after_its_last_step() {
    let script = include_str!("../showcase/record.mjs");
    let steps = script.find("else await steps(page, scene);").unwrap();
    let hold = script
        .find("const hold = scene.kind === \"browser\" ? Math.max(scene.seconds * 1000, left) : left;")
        .unwrap();
    let sleep = script.find("if (hold > 0) await sleep(hold);").unwrap();
    assert!(steps < hold && hold < sleep);
    assert!(!script.contains("if (left > 0) await sleep(left);"));
}

#[test]
fn the_caption_is_drawn_again_after_every_main_frame_navigation() {
    let script = include_str!("../showcase/record.mjs");
    assert!(script.contains("page.on(\"framenavigated\", (frame) => {"));
    assert!(script.contains(
        "if (frame === page.mainFrame() && caption !== null) frame.evaluate(overlay, caption).catch(() => {});"
    ));
    // One caption at a time, drawn once the new document has a body.
    assert!(script.contains("document.getElementById(\"showcase-caption\")?.remove();"));
    assert!(
        script.contains("document.addEventListener(\"DOMContentLoaded\", draw, { once: true });")
    );
    assert!(script.contains("caption = scene.caption;"));
    assert!(script.contains("caption = null;"));
    let listener = script.find("page.on(\"framenavigated\"").unwrap();
    let first_scene = script.find("for (const scene of plan.scenes)").unwrap();
    assert!(listener < first_scene);
}

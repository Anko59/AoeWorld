//! Static files are gzip-encoded by the server the first time a request that
//! allows gzip asks for them; the encoded bytes are then reused for as long as
//! the file keeps its `Last-Modified` and `Content-Length`. Anything that is not
//! a compressible `200` answer to such a GET passes through as it was.
use aoe_scenario::SMOKE;
use aoe_server::{AppState, Config, app};
use std::{
    io::Read,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::OnceLock,
    time::{Duration, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

const GZIP: &str = "Accept-Encoding: gzip";
/// What a browser sends to a secure origin; over plain HTTP it is `gzip, deflate`.
const BROWSER: &str = "Accept-Encoding: gzip, deflate, br, zstd";
const KIB: usize = 1024;
/// Every fixture file starts with this date, in seconds after the Unix epoch.
const MODIFIED: u64 = 1_700_000_000;

/// Fixture files: path under the server root, content seed, length in bytes.
/// All hold compressible text, so only a file's type or size decides. The three
/// `manifest.json` share a name, a length and a date, and differ in content.
const FILES: [(&str, u8, usize); 14] = [
    ("web/index.html", 1, 2 * KIB),
    ("web/pkg/a.wasm", 2, 100 * KIB),
    ("web/pkg/a.js", 3, 100 * KIB),
    ("web/pkg/terrain.wgsl", 4, 2 * KIB),
    ("web/interface/game.css", 5, 2 * KIB),
    ("web/interface/icons.svg", 6, 2 * KIB),
    ("web/edge.js", 7, KIB),
    ("web/small.js", 8, KIB - 1),
    ("web/art/icon.png", 9, 4 * KIB),
    ("web/manifest.json", 10, 4 * KIB),
    ("pack/manifest.json", 11, 4 * KIB),
    ("local-assets/series-art/manifest.json", 12, 4 * KIB),
    ("pack/terrain.bin", 13, 4 * KIB),
    ("pack/color-0.png", 14, 4 * KIB),
];

/// `length` bytes of repetitive text; another `seed` gives other bytes.
fn text(seed: u8, length: usize) -> Vec<u8> {
    let line = format!("const value_{seed} = \"static fixture {seed}\";\n");
    line.bytes().cycle().take(length).collect()
}

fn write(file: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, bytes).unwrap();
}

/// Writes `bytes` and dates the file `seconds` after the Unix epoch.
fn write_at(file: &Path, bytes: &[u8], seconds: u64) {
    write(file, bytes);
    let modified = UNIX_EPOCH + Duration::from_secs(seconds);
    let handle = std::fs::File::options().write(true).open(file).unwrap();
    handle.set_modified(modified).unwrap();
}

/// The server resolves `web` and `local-assets/series-art` against its working
/// directory, so this process enters one temporary server root, once.
fn server_root() -> &'static PathBuf {
    static ROOT: OnceLock<PathBuf> = OnceLock::new();
    ROOT.get_or_init(|| {
        let root = tempfile::tempdir().unwrap().keep();
        for (path, seed, length) in FILES {
            write_at(&root.join(path), &text(seed, length), MODIFIED);
        }
        std::env::set_current_dir(&root).unwrap();
        root
    })
}

/// The file the server reads for a URL.
fn on_disk(url: &str) -> PathBuf {
    let root = server_root();
    if let Some(rest) = url.strip_prefix("/asset-pack/") {
        root.join("pack").join(rest)
    } else if let Some(rest) = url.strip_prefix("/series-art/") {
        root.join("local-assets/series-art").join(rest)
    } else if url == "/" {
        root.join("web/index.html")
    } else {
        root.join("web").join(&url[1..])
    }
}

async fn serve() -> (SocketAddr, AppState) {
    let pack = server_root().join("pack");
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let state = AppState::new(
        &Config {
            bind: address,
            scenario: SMOKE,
            tick_hz: 20,
            asset_pack: Some(pack),
            map_package_directory: None,
            open_map_package: None,
            map_worker: None,
            geodata_cache_directory: ".cache/geodata".into(),
        },
        "static-files-test",
    )
    .unwrap();
    let router = app(state.clone());
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (address, state)
}

/// The seam: how many static responses this server has run the gzip encoder on.
/// A response served from the cache must not count.
fn compressions(state: &AppState) -> u64 {
    state.static_compressions()
}

struct Reply {
    /// The request, then the status line and headers it got.
    seen: String,
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

async fn request(address: SocketAddr, method: &str, path: &str, extra: &[&str]) -> Reply {
    let mut message = format!("{method} {path} HTTP/1.1\r\nHost: localhost\r\n");
    for line in extra.iter().chain(&["Connection: close", ""]) {
        message.push_str(line);
        message.push_str("\r\n");
    }
    let mut stream = TcpStream::connect(address).await.unwrap();
    stream.write_all(message.as_bytes()).await.unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.unwrap();
    let end = bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("a complete header block");
    let head = String::from_utf8_lossy(&bytes[..end]).into_owned();
    let mut lines = head.split("\r\n");
    let status = lines.next().unwrap().split(' ').nth(1).unwrap();
    let headers = lines
        .map(|line| {
            let (name, value) = line.split_once(':').expect("a header line");
            (name.to_ascii_lowercase(), value.trim().to_owned())
        })
        .collect();
    Reply {
        seen: format!("{method} {path} {extra:?} got:\n{head}"),
        status: status.parse().unwrap(),
        headers,
        body: bytes.split_off(end + 4),
    }
}

impl Reply {
    fn all(&self, name: &str) -> Vec<&str> {
        let values = self.headers.iter().filter(|(key, _)| key == name);
        values.map(|(_, value)| value.as_str()).collect()
    }

    fn varies_on_accept_encoding(&self) -> bool {
        self.all("vary").iter().any(|value| {
            let mut names = value.split(',');
            names.any(|name| name.trim().eq_ignore_ascii_case("accept-encoding"))
        })
    }

    /// The bytes a client ends up with, once a gzip coding is undone.
    fn decoded(&self) -> Vec<u8> {
        if self.all("content-encoding") != ["gzip"] {
            return self.body.clone();
        }
        let mut bytes = Vec::new();
        let mut decoder = flate2::read::GzDecoder::new(&self.body[..]);
        decoder.read_to_end(&mut bytes).expect(&self.seen);
        bytes
    }

    /// Passed through: the file itself, with its own length and no coding.
    #[track_caller]
    fn assert_raw(&self, file: &[u8]) {
        let seen = &self.seen;
        let sent = self.body.len();
        let length = file.len().to_string();
        assert_eq!(self.status, 200, "{seen}");
        assert!(self.all("content-encoding").is_empty(), "{seen}");
        assert!(self.all("transfer-encoding").is_empty(), "{seen}");
        assert_eq!(self.all("content-length"), [length], "{seen}");
        assert_eq!(self.all("cache-control"), ["no-cache"], "{seen}");
        assert!(self.body == file, "{seen}\n\nbody: {sent} bytes");
    }

    /// Encoded: a smaller gzip body that decodes to `file` and says so, with the
    /// type and date of `plain`, the same URL asked for without negotiation.
    #[track_caller]
    fn assert_gzip(&self, file: &[u8], plain: &Reply) {
        let seen = &self.seen;
        let sent = self.body.len();
        assert_eq!(self.status, 200, "{seen}");
        assert_eq!(self.all("content-encoding"), ["gzip"], "{seen}");
        for kept in ["content-type", "last-modified"] {
            assert_eq!(self.all(kept).len(), 1, "{seen}");
            assert_eq!(self.all(kept), plain.all(kept), "{seen}");
        }
        assert_eq!(self.all("cache-control"), ["no-cache"], "{seen}");
        assert!(self.varies_on_accept_encoding(), "no Vary: {seen}");
        assert!(self.all("transfer-encoding").is_empty(), "{seen}");
        assert_eq!(self.all("content-length"), [sent.to_string()], "{seen}");
        assert!(sent < file.len(), "{seen}\n\nbody: {sent} bytes");
        assert!(self.decoded() == file, "{seen}\n\ndecodes to other bytes");
    }
}

/// (a) The count: a 100 KiB module or script crosses the wire as a smaller gzip
/// body that decodes to the file, however the request allows gzip, and the
/// encoder ran once per file.
#[tokio::test]
async fn static_wasm_and_js_are_sent_gzip_encoded() {
    let (address, state) = serve().await;
    for url in ["/pkg/a.wasm", "/pkg/a.js"] {
        let file = std::fs::read(on_disk(url)).unwrap();
        assert_eq!(file.len(), 100 * KIB);
        let plain = request(address, "GET", url, &[]).await;
        plain.assert_raw(&file);
        for accept in [GZIP, BROWSER, "Accept-Encoding: gzip;q=0.5"] {
            let reply = request(address, "GET", url, &[accept]).await;
            reply.assert_gzip(&file, &plain);
        }
    }
    let wasm = request(address, "GET", "/pkg/a.wasm", &[GZIP]).await;
    assert_eq!(wasm.all("content-type"), ["application/wasm"]);
    assert_eq!(compressions(&state), 2);
}

/// The other text types and all three served directories, down to exactly
/// 1 KiB. `.wgsl` is text whatever type `ServeDir` guesses for it. The three
/// `manifest.json` only differ by directory: each must decode to its own bytes.
#[tokio::test]
async fn static_text_types_in_every_served_directory_are_sent_gzip_encoded() {
    let (address, state) = serve().await;
    let urls = [
        "/manifest.json",
        "/asset-pack/manifest.json",
        "/series-art/manifest.json",
        "/",
        "/interface/game.css",
        "/interface/icons.svg",
        "/pkg/terrain.wgsl",
        "/edge.js",
    ];
    for url in urls {
        let file = std::fs::read(on_disk(url)).unwrap();
        let plain = request(address, "GET", url, &[]).await;
        plain.assert_raw(&file);
        let reply = request(address, "GET", url, &[GZIP]).await;
        reply.assert_gzip(&file, &plain);
    }
    assert_eq!(compressions(&state), 8);
}

/// A compressible file sent raw, because the request did not allow gzip, still
/// says that its representation depends on `Accept-Encoding`.
#[tokio::test]
async fn static_compressible_file_sent_raw_varies_on_accept_encoding() {
    let (address, state) = serve().await;
    let file = std::fs::read(on_disk("/pkg/a.wasm")).unwrap();
    for extra in [&[][..], &["Accept-Encoding: gzip;q=0"][..]] {
        let reply = request(address, "GET", "/pkg/a.wasm", extra).await;
        reply.assert_raw(&file);
        let varies = reply.varies_on_accept_encoding();
        assert!(varies, "no Vary: {}", reply.seen);
    }
    assert_eq!(compressions(&state), 0);
}

/// (e) The count: the encoder runs once per version of a file, not once per
/// request. The repeated replies are the cached bytes.
#[tokio::test]
async fn static_second_identical_request_does_not_compress_again() {
    let (address, state) = serve().await;
    let url = "/pkg/counted.js";
    write_at(&on_disk(url), &text(1, 8 * KIB), MODIFIED);
    assert_eq!(compressions(&state), 0);
    let plain = request(address, "GET", url, &[]).await;
    let first = request(address, "GET", url, &[GZIP]).await;
    first.assert_gzip(&text(1, 8 * KIB), &plain);
    assert_eq!(compressions(&state), 1);
    for accept in [GZIP, BROWSER, GZIP] {
        let again = request(address, "GET", url, &[accept]).await;
        assert!(again.body == first.body, "{}", again.seen);
    }
    assert_eq!(compressions(&state), 1, "a cache hit ran the encoder");

    write_at(&on_disk(url), &text(2, 8 * KIB), MODIFIED + 10);
    for _ in 0..2 {
        let reply = request(address, "GET", url, &[GZIP]).await;
        assert!(reply.decoded() == text(2, 8 * KIB), "{}", reply.seen);
    }
    assert_eq!(compressions(&state), 2);
}

/// (b) A request that does not allow gzip gets the file itself: no such header,
/// a zero weight however it is written, or other codings only.
#[tokio::test]
async fn static_compressible_file_is_sent_raw_to_requests_not_allowing_gzip() {
    let (address, state) = serve().await;
    for url in ["/pkg/a.wasm", "/pkg/a.js"] {
        let file = std::fs::read(on_disk(url)).unwrap();
        for accept in [
            "Accept: */*",
            "Accept-Encoding: identity",
            "Accept-Encoding: gzip;q=0",
            "Accept-Encoding: gzip; q=0",
            "Accept-Encoding: gzip;q=0.000",
            "Accept-Encoding: br",
            "Accept-Encoding: deflate, zstd",
        ] {
            let reply = request(address, "GET", url, &[accept]).await;
            reply.assert_raw(&file);
        }
    }
    assert_eq!(compressions(&state), 0);
}

/// (c) Requests the encoder leaves alone although they allow gzip and name a
/// compressible file: a range, a HEAD, and a revalidation answered 304.
#[tokio::test]
async fn static_range_head_and_revalidation_requests_are_untouched() {
    let (address, state) = serve().await;
    let url = "/pkg/a.wasm";
    let file = std::fs::read(on_disk(url)).unwrap();
    let length = file.len().to_string();
    let span = format!("bytes 10-109/{length}");

    let range = request(address, "GET", url, &[GZIP, "Range: bytes=10-109"]).await;
    assert_eq!(range.status, 206, "{}", range.seen);
    assert_eq!(range.all("content-range"), [span], "{}", range.seen);
    assert_eq!(range.all("content-length"), ["100"], "{}", range.seen);
    assert!(range.all("content-encoding").is_empty(), "{}", range.seen);
    assert!(range.body == file[10..110], "{}", range.seen);

    let head = request(address, "HEAD", url, &[GZIP]).await;
    assert_eq!(head.status, 200, "{}", head.seen);
    assert_eq!(head.all("content-length"), [length], "{}", head.seen);
    assert!(head.all("content-encoding").is_empty(), "{}", head.seen);
    assert!(head.body.is_empty(), "{}", head.seen);

    let plain = request(address, "GET", url, &[]).await;
    let since = format!("If-Modified-Since: {}", plain.all("last-modified")[0]);
    let fresh = request(address, "GET", url, &[GZIP, since.as_str()]).await;
    assert_eq!(fresh.status, 304, "{}", fresh.seen);
    assert!(fresh.all("content-encoding").is_empty(), "{}", fresh.seen);
    assert_eq!(fresh.all("cache-control"), ["no-cache"], "{}", fresh.seen);
    assert!(fresh.body.is_empty(), "{}", fresh.seen);
    assert_eq!(compressions(&state), 0);
}

/// (c) Images and chunks are never encoded: the client compares a chunk's
/// `Content-Length` with its expected size (`native/fetch.js`). A missing file
/// stays a plain 404.
#[tokio::test]
async fn static_png_and_bin_are_never_encoded() {
    let (address, state) = serve().await;
    for url in [
        "/asset-pack/terrain.bin",
        "/asset-pack/color-0.png",
        "/art/icon.png",
    ] {
        let file = std::fs::read(on_disk(url)).unwrap();
        for accept in [GZIP, BROWSER] {
            let reply = request(address, "GET", url, &[accept]).await;
            reply.assert_raw(&file);
        }
    }
    let gone = request(address, "GET", "/pkg/missing.js", &[GZIP]).await;
    assert_eq!(gone.status, 404, "{}", gone.seen);
    assert!(gone.all("content-encoding").is_empty(), "{}", gone.seen);
    assert_eq!(compressions(&state), 0);
}

/// (f) Size bounds: one byte under 1 KiB, or one byte over 16 MiB, and a file
/// of a compressible type is not encoded.
#[tokio::test]
async fn static_files_outside_the_size_bounds_are_not_encoded() {
    let (address, state) = serve().await;
    let small = std::fs::read(on_disk("/small.js")).unwrap();
    let huge = vec![b' '; 16 * KIB * KIB + 1];
    write(&on_disk("/huge.json"), &huge);
    assert_eq!(small.len(), KIB - 1);
    for (url, file) in [("/small.js", &small), ("/huge.json", &huge)] {
        let reply = request(address, "GET", url, &[GZIP]).await;
        reply.assert_raw(file);
    }
    assert_eq!(compressions(&state), 0);
}

/// (d) A rebuilt file is never served from a stale entry: a later date with the
/// same length, then another length with the same date, each give the new bytes,
/// on the first request and on the next.
#[tokio::test]
async fn static_rewritten_file_is_sent_with_its_new_bytes() {
    let (address, _state) = serve().await;
    let url = "/pkg/rebuilt.js";
    for (seed, length, seconds) in [
        (1, 4 * KIB, MODIFIED),
        (2, 4 * KIB, MODIFIED + 10),
        (3, 5 * KIB, MODIFIED + 10),
    ] {
        let file = text(seed, length);
        write_at(&on_disk(url), &file, seconds);
        for _ in 0..2 {
            let reply = request(address, "GET", url, &[GZIP]).await;
            assert_eq!(reply.status, 200, "{}", reply.seen);
            assert!(reply.decoded() == file, "{}", reply.seen);
        }
    }
}

#[path = "static_files/abandoned.rs"]
mod abandoned;
#[path = "static_files/limits.rs"]
mod limits;
#[path = "static_files/negotiation.rs"]
mod negotiation;
#[path = "static_files/spellings.rs"]
mod spellings;

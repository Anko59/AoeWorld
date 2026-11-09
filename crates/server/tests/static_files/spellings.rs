//! One file has many spellings: `ServeDir` skips empty and `.` segments and
//! undoes percent escapes. Only the plain spelling is encoded and kept. A path
//! with a `%`, an empty segment before its end or a `.` segment gets the file
//! itself, as `ServeDir` answers it, so that no request can make the server
//! read and gzip an unchanged file again.
use super::*;

const WASM: &str = "/pkg/a.wasm";

/// Other ways to ask for `/pkg/a.wasm`.
const ODD: [&str; 10] = [
    "/pkg//a.wasm",
    "//pkg/a.wasm",
    "/pkg/a.wasm//",
    "/pkg/./a.wasm",
    "/./pkg/a.wasm",
    "/pkg/a.wasm/.",
    "/pkg/%61.wasm",
    "/pkg/a%2Ewasm",
    "/pkg%2Fa.wasm",
    "/%70%6b%67/a.wasm",
];

/// Each of them, then as many more as a client cares to send.
fn odd_spellings() -> Vec<String> {
    let mut spellings = Vec::from(ODD.map(String::from));
    for more in 3..12 {
        spellings.push(format!("/pkg{}a.wasm", "/".repeat(more)));
        spellings.push(format!("/pkg/{}a.wasm", "./".repeat(more)));
    }
    spellings
}

/// The reply to each odd spelling, asked for with the header lines `extra`.
async fn ask_all(address: SocketAddr, extra: &[&str]) -> Vec<Reply> {
    let mut replies = Vec::new();
    for spelling in odd_spellings() {
        replies.push(request(address, "GET", &spelling, extra).await);
    }
    replies
}

/// The count: once the plain spelling is encoded, no other spelling of the file
/// runs the encoder again, however many are tried, and each gets the file itself.
#[tokio::test]
async fn static_odd_spellings_of_an_encoded_file_are_sent_raw() {
    let (address, state) = serve().await;
    let file = std::fs::read(on_disk(WASM)).unwrap();
    let plain = request(address, "GET", WASM, &[]).await;
    let first = request(address, "GET", WASM, &[GZIP]).await;
    first.assert_gzip(&file, &plain);
    assert_eq!(compressions(&state), 1);

    // `ServeDir` does resolve every one of them to that file.
    for reply in ask_all(address, &[]).await {
        reply.assert_raw(&file);
    }
    let mut replies = ask_all(address, &[GZIP]).await;
    replies.extend(ask_all(address, &[BROWSER]).await);
    assert_eq!(compressions(&state), 1, "another spelling ran the encoder");
    for reply in replies {
        reply.assert_raw(&file);
    }
    let again = request(address, "GET", WASM, &[GZIP]).await;
    assert!(again.body == first.body, "{}", again.seen);
    assert_eq!(compressions(&state), 1);
}

/// The count: odd spellings alone never run the encoder. The plain spelling
/// then does, once.
#[tokio::test]
async fn static_odd_spellings_alone_never_run_the_encoder() {
    let (address, state) = serve().await;
    let file = std::fs::read(on_disk(WASM)).unwrap();
    let replies = ask_all(address, &[GZIP]).await;
    assert_eq!(compressions(&state), 0, "an odd spelling ran the encoder");
    for reply in replies {
        reply.assert_raw(&file);
    }
    let plain = request(address, "GET", WASM, &[]).await;
    let reply = request(address, "GET", WASM, &[GZIP]).await;
    reply.assert_gzip(&file, &plain);
    assert_eq!(compressions(&state), 1);
}

/// The same under the nested directories, whose layer sees the path without
/// its mount. One slash too many right after the mount is not odd there: axum
/// strips it with the mount, and that request is the plain one.
#[tokio::test]
async fn static_odd_spellings_under_a_nested_directory_are_sent_raw() {
    let (address, state) = serve().await;
    for (encoded, mount) in [(1, "/asset-pack"), (2, "/series-art")] {
        let url = format!("{mount}/manifest.json");
        let file = std::fs::read(on_disk(&url)).unwrap();
        let plain = request(address, "GET", &url, &[]).await;
        let first = request(address, "GET", &url, &[GZIP]).await;
        first.assert_gzip(&file, &plain);
        let odd = [
            "/./manifest.json",
            "///manifest.json",
            "/%6Danifest.json",
            "/manifest%2Ejson",
        ];
        let mut replies = Vec::new();
        for spelling in odd.map(|rest| format!("{mount}{rest}")) {
            let unasked = request(address, "GET", &spelling, &[]).await;
            unasked.assert_raw(&file);
            replies.push(request(address, "GET", &spelling, &[GZIP]).await);
        }
        let stripped = format!("{mount}//manifest.json");
        let stripped = request(address, "GET", &stripped, &[GZIP]).await;
        assert_eq!(
            compressions(&state),
            encoded,
            "another spelling ran the encoder"
        );
        for reply in replies {
            reply.assert_raw(&file);
        }
        assert!(stripped.decoded() == file, "{}", stripped.seen);
    }
}

/// A query string and a trailing slash are not odd. The query is no part of
/// the path, so its reply is the kept one. The slash names a directory's index
/// page, which is encoded like any file. After a file's name `ServeDir` ignores
/// it, and that spelling may be kept as one more copy: made once, never once
/// per request.
#[tokio::test]
async fn static_query_string_and_trailing_slash_are_not_odd() {
    let (address, state) = serve().await;
    let page = text(1, 2 * KIB);
    write_at(&on_disk("/guide/index.html"), &page, MODIFIED);
    let plain = request(address, "GET", "/guide/", &[]).await;
    plain.assert_raw(&page);
    let index = request(address, "GET", "/guide/", &[GZIP]).await;
    index.assert_gzip(&page, &plain);

    let file = std::fs::read(on_disk(WASM)).unwrap();
    for url in [WASM, "/pkg/a.wasm?v=1", "/pkg/a.wasm?v=2"] {
        let reply = request(address, "GET", url, &[GZIP]).await;
        assert_eq!(reply.all("content-encoding"), ["gzip"], "{}", reply.seen);
        assert!(reply.decoded() == file, "{}", reply.seen);
    }
    assert_eq!(compressions(&state), 2);
    for _ in 0..4 {
        let reply = request(address, "GET", "/pkg/a.wasm/", &[GZIP]).await;
        assert_eq!(reply.status, 200, "{}", reply.seen);
        assert!(reply.decoded() == file, "{}", reply.seen);
    }
    let more = compressions(&state) - 2;
    assert!(more <= 1, "a trailing slash ran the encoder {more} times");
}

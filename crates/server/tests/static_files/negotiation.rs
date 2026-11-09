//! The `Accept-Encoding` decision, case by case, and what an encoded reply says
//! about ranges.
use super::*;

/// Request header lines, and whether they allow a gzip reply. Two lines of one
/// name are one list.
const CASES: [(&[&str], bool); 17] = [
    (&["Accept-Encoding: gzip"], true),
    (&["Accept-Encoding: GZIP"], true),
    (&["Accept-Encoding: x-gzip"], true),
    (&["Accept-Encoding: gzip;q=0.5"], true),
    (&["Accept-Encoding: *"], true),
    (&["Accept-Encoding: *;q=0, gzip"], true),
    (&["Accept-Encoding: br", "Accept-Encoding: gzip"], true),
    (&[], false),
    (&["Accept-Encoding:"], false),
    (&["Accept-Encoding: identity"], false),
    (&["Accept-Encoding: br"], false),
    (&["Accept-Encoding: gzip;q=0"], false),
    (&["Accept-Encoding: gzip; q=0"], false),
    (&["Accept-Encoding: *;q=0"], false),
    (&["Accept-Encoding: gzip;q=0, *"], false),
    (&["Accept-Encoding: gzip;q=high"], false),
    (&["Accept-Encoding: gzip;q=0", "Accept-Encoding: *"], false),
];

/// Every case through the router, for one file: its gzip body or the file
/// itself, either one marked as depending on `Accept-Encoding`, and one run of
/// the encoder for them all.
#[tokio::test]
async fn static_accept_encoding_decides_between_gzip_and_the_file() {
    let (address, state) = serve().await;
    let url = "/pkg/a.js";
    let file = std::fs::read(on_disk(url)).unwrap();
    let plain = request(address, "GET", url, &[]).await;
    for (lines, allowed) in CASES {
        let reply = request(address, "GET", url, lines).await;
        if allowed {
            reply.assert_gzip(&file, &plain);
        } else {
            reply.assert_raw(&file);
        }
        let varies = reply.varies_on_accept_encoding();
        assert!(varies, "no Vary: {}", reply.seen);
    }
    assert_eq!(compressions(&state), 1);
}

/// Ranges are served from the file only: an encoded reply does not offer them,
/// made now or kept, while the file itself does, and a range request still gets
/// the file's bytes once an encoding is kept.
#[tokio::test]
async fn static_encoded_reply_does_not_offer_ranges() {
    let (address, state) = serve().await;
    let url = "/pkg/a.wasm";
    let file = std::fs::read(on_disk(url)).unwrap();
    let plain = request(address, "GET", url, &[]).await;
    assert_eq!(plain.all("accept-ranges"), ["bytes"], "{}", plain.seen);
    for _ in 0..2 {
        let reply = request(address, "GET", url, &[GZIP]).await;
        reply.assert_gzip(&file, &plain);
        assert!(reply.all("accept-ranges").is_empty(), "{}", reply.seen);
    }
    let range = request(address, "GET", url, &[GZIP, "Range: bytes=10-109"]).await;
    assert_eq!(range.status, 206, "{}", range.seen);
    assert!(range.all("content-encoding").is_empty(), "{}", range.seen);
    assert!(range.body == file[10..110], "{}", range.seen);
    assert_eq!(compressions(&state), 1);
}

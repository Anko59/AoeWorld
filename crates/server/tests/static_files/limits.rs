//! The limits of what is encoded: a file of exactly the largest size still is,
//! and a file that gzip does not make smaller is sent as it is.
use super::*;
use std::io::Write;

/// The upper bound is inclusive: exactly 16 MiB is encoded. One byte more is
/// not (`static_files_outside_the_size_bounds_are_not_encoded`).
#[tokio::test]
async fn static_file_of_exactly_the_largest_size_is_encoded() {
    let (address, state) = serve().await;
    let url = "/full.json";
    let file = text(1, 16 * KIB * KIB);
    write_at(&on_disk(url), &file, MODIFIED);
    let plain = request(address, "GET", url, &[]).await;
    plain.assert_raw(&file);
    let reply = request(address, "GET", url, &[GZIP]).await;
    reply.assert_gzip(&file, &plain);
    assert_eq!(compressions(&state), 1);
}

const NOISE: &str = "/pkg/noise.js";

/// Writes `NOISE`, a script of random-looking bytes (a fixed xorshift
/// sequence), and returns them. Even the best gzip body is not smaller.
fn noise() -> Vec<u8> {
    let mut state = 0x9E37_79B9_7F4A_7C15_u64;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state >> 32) as u8
    };
    let file: Vec<u8> = (0..8 * KIB).map(|_| next()).collect();
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    encoder.write_all(&file).unwrap();
    let smallest = encoder.finish().unwrap().len();
    assert!(smallest >= file.len(), "the fixture shrinks to {smallest}");
    write_at(&on_disk(NOISE), &file, MODIFIED);
    file
}

/// A text type does not make a file compressible: when its gzip body would not
/// be smaller than the file, the file itself is sent, on the first request and
/// on the next.
#[tokio::test]
async fn static_file_that_gzip_does_not_shrink_is_sent_raw() {
    let (address, _state) = serve().await;
    let file = noise();
    for _ in 0..2 {
        let reply = request(address, "GET", NOISE, &[GZIP]).await;
        reply.assert_raw(&file);
    }
}

/// The count: finding that out costs at most one run of the encoder for that
/// version of the file, not one per request.
#[tokio::test]
async fn static_file_that_gzip_does_not_shrink_is_encoded_at_most_once() {
    let (address, state) = serve().await;
    let file = noise();
    for _ in 0..3 {
        let reply = request(address, "GET", NOISE, &[GZIP]).await;
        assert_eq!(reply.status, 200, "{}", reply.seen);
        assert!(reply.decoded() == file, "{}", reply.seen);
    }
    let runs = compressions(&state);
    assert!(runs <= 1, "the encoder ran {runs} times on one file");
}

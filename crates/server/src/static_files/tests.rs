//! The cache's own cap, on the cache itself: what it keeps never weighs more
//! than its limit, the entries used longest ago go first, and a rebuilt file
//! gives the bytes of its older version back.
use super::*;

/// Every body is this long. Two of them fit under `LIMIT` with all their
/// bookkeeping; three never do.
const BODY: usize = 10_000;
const LIMIT: usize = 25_000;

fn key(name: &str) -> Key {
    ("", format!("/pkg/{name}.js"))
}

fn version(build: u32) -> Version {
    Version {
        modified: HeaderValue::from_static("Tue, 14 Nov 2023 22:13:20 GMT"),
        length: build,
    }
}

/// Keeps a body filled with `fill` as the encoding of `build` of `name`.
fn keep(cache: &Cache, name: &str, build: u32, fill: u8) {
    cache.insert(key(name), version(build), Bytes::from(vec![fill; BODY]));
}

/// The fill of the body kept for `build` of `name`, if one is.
fn kept(cache: &Cache, name: &str, build: u32) -> Option<u8> {
    let body = cache.get(&key(name), &version(build))?;
    assert_eq!(body.len(), BODY);
    Some(body[0])
}

/// Six bodies where two fit: the four oldest are gone, and the two kept weigh
/// no more than the limit.
#[test]
fn cap_drops_the_oldest_entries_and_stays_within_the_limit() {
    let cache = Cache::with_limit(LIMIT);
    let names = ["a", "b", "c", "d", "e", "f"];
    for (fill, name) in (0..).zip(names) {
        keep(&cache, name, 1, fill);
    }
    let left = names.map(|name| kept(&cache, name, 1));
    assert_eq!(left, [None, None, None, None, Some(4), Some(5)]);
    assert!(left.iter().flatten().count() * BODY <= LIMIT);
}

/// "Oldest" is by use, not by arrival: a body that was just served outlives
/// one that was not.
#[test]
fn cap_drops_the_entry_used_longest_ago() {
    let cache = Cache::with_limit(LIMIT);
    keep(&cache, "a", 1, 1);
    keep(&cache, "b", 1, 2);
    assert_eq!(kept(&cache, "a", 1), Some(1));
    keep(&cache, "c", 1, 3);
    assert_eq!(kept(&cache, "b", 1), None);
    assert_eq!(kept(&cache, "a", 1), Some(1));
    assert_eq!(kept(&cache, "c", 1), Some(3));
}

/// A rebuilt file takes the place of its older version, which is neither
/// served nor charged any more: twenty builds of one file do not push another
/// file out.
#[test]
fn cap_charges_a_rebuilt_file_once() {
    let cache = Cache::with_limit(LIMIT);
    keep(&cache, "a", 1, 1);
    for build in 1..=20 {
        keep(&cache, "b", build, 2);
    }
    assert_eq!(kept(&cache, "b", 19), None);
    assert_eq!(kept(&cache, "b", 20), Some(2));
    assert_eq!(kept(&cache, "a", 1), Some(1));
}

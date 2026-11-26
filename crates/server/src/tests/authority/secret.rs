//! Where secrets come from and how they are compared. Randomness and timing are
//! no property of a few outputs, so these read the sources: one constructor of
//! tokens, one reader of the hosting secret.
use super::*;
use serde_json::json;
use std::{fs, path::Path};

/// This crate's production sources: their paths and texts.
fn sources() -> Vec<(String, String)> {
    fn visit(directory: &Path, found: &mut Vec<(String, String)>) {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_str().unwrap();
            if path.is_dir() {
                if name != "tests" {
                    visit(&path, found);
                }
            } else if name.ends_with(".rs") && name != "tests.rs" && !name.ends_with("_tests.rs") {
                let text = fs::read_to_string(&path).unwrap();
                found.push((path.display().to_string(), text));
            }
        }
    }
    let mut found = Vec::new();
    visit(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut found,
    );
    found
}

/// A token names its secret source: the operating system's, in the file that
/// makes tokens.
#[test]
fn tokens_come_from_the_operating_systems_random_source() {
    let sources = sources();
    let mut makers = sources
        .iter()
        .filter(|(_, text)| text.contains("fn make_token"));
    let (path, text) = makers.next().expect("make_token, the token constructor");
    assert!(makers.next().is_none(), "one constructor of tokens");
    let predictable = ["RandomState", "DefaultHasher", "SystemTime", "thread_rng"];
    for source in predictable {
        assert!(!text.contains(source), "{path}: a token is from {source}");
    }
    let secret = ["getrandom", "OsRng", "SysRng", "/dev/urandom"];
    assert!(
        secret.iter().any(|source| text.contains(source)),
        "{path}: tokens name none of {secret:?}"
    );
}

/// One function reads the hosting secret a request presents, for every route,
/// and compares it with a `same_...` helper as tokens are, never with `==`.
#[test]
fn the_hosting_secret_is_read_once_and_compared_as_tokens_are() {
    let mut readers = Vec::new();
    for (path, text) in sources() {
        for (at, _) in text.match_indices(".get(ADMIN_TOKEN_HEADER)") {
            let rest = &text[at..];
            let end = rest.find("\n}").unwrap_or(rest.len());
            readers.push((path.clone(), rest[..end].to_owned()));
        }
    }
    let paths: Vec<_> = readers.iter().map(|(path, _)| path).collect();
    assert_eq!(readers.len(), 1, "the secret is read in {paths:?}");
    let (path, body) = &readers[0];
    assert!(!body.contains("=="), "{path}: `==` tells a guess's prefix");
    assert!(body.contains("same_"), "{path}: not compared as tokens are");
}

/// Whatever compares it, only the whole secret passes: no prefix, no longer
/// text, no other case and no byte changed.
#[tokio::test]
async fn only_the_whole_hosting_secret_passes() {
    let server = serve(villages(1)).await;
    let mut wrong = vec![
        String::new(),
        SECRET[..1].to_owned(),
        SECRET[..SECRET.len() - 1].to_owned(),
        SECRET[1..].to_owned(),
        format!("{SECRET}0"),
        format!("{SECRET}{SECRET}"),
        SECRET.to_uppercase(),
    ];
    for at in 0..SECRET.len() {
        let mut near = SECRET.to_owned().into_bytes();
        near[at] ^= 1;
        wrong.push(String::from_utf8(near).unwrap());
    }
    let step = json!({"ticks": 1, "orders": 0});
    let asks = [
        // Without a map, a permitted match is a bad request; without the arena,
        // a permitted step is unknown. Neither changes the served world.
        (
            Ask::new("POST", "/match").json(&json!({"slots": ["ai"]})),
            400,
        ),
        (Ask::new("POST", "/match/step").json(&step), 404),
        (Ask::new("GET", "/match/stats?since=0"), 200),
        (Ask::new("GET", "/admin/match"), 200),
    ];
    for (ask, permitted) in asks {
        assert_eq!(ask.clone().send(server.address).await.0, 403);
        for secret in &wrong {
            let guess = ask.clone().with_secret(secret);
            let (status, _) = guess.send(server.address).await;
            assert_eq!(status, 403, "{} {}: {secret:?}", ask.method, ask.path);
        }
        let right = ask.clone().with_secret(SECRET);
        let (status, _) = right.send(server.address).await;
        assert_eq!(status, permitted, "{} {}", ask.method, ask.path);
    }
}

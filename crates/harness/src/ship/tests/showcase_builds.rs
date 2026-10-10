//! Before-and-after browser scenes: the storyboard's `build`, the second
//! origin (SHOWCASE_BEFORE_URL), each scene's URL and the per-scene filter.
use super::{
    super::showcase::{AppOrigin, Build, Origins, Scene, Storyboard, check, make, plan},
    showcase_pipeline::{Environment, mode},
    showcase_security::ENV_LOCK,
};

const APP: &str = "http://127.0.0.1:8080/";
const BEFORE: &str = "http://127.0.0.1:8082/";

fn scene(build: &str, path: &str) -> String {
    format!(r#"{{"kind":"browser",{build}"path":"{path}","caption":"The lab","seconds":3}}"#)
}

fn board(scenes: &[String]) -> String {
    format!(
        r#"{{"title":"Lab","scenes":[{{"kind":"card","heading":"Opening"}},{}]}}"#,
        scenes.join(",")
    )
}

fn both() -> String {
    board(&[
        scene(r#""build":"before","#, "/lab?seed=4"),
        scene(r#""build":"after","#, "/lab?seed=4"),
        scene("", "/"),
    ])
}

fn builds(board: &Storyboard) -> Vec<Build> {
    board
        .scenes
        .iter()
        .filter_map(|scene| match scene {
            Scene::Browser { build, .. } => Some(*build),
            _ => None,
        })
        .collect()
}

#[test]
fn a_browser_scene_names_its_build_and_defaults_to_after() {
    let parsed = Storyboard::parse(&both()).unwrap();
    assert_eq!(builds(&parsed), [Build::Before, Build::After, Build::After]);
    assert_eq!(Build::default(), Build::After);
    for value in [
        r#""Before""#,
        r#""BEFORE""#,
        r#""dev""#,
        r#""""#,
        "null",
        "1",
    ] {
        let text = board(&[scene(&format!(r#""build":{value},"#), "/")]);
        assert!(Storyboard::parse(&text).is_err(), "{value}");
    }
    // Only browser scenes have a build: terminals keep `tone`.
    let card = r#"{"title":"x","scenes":[{"kind":"card","heading":"a","build":"before"}]}"#;
    assert!(Storyboard::parse(card).is_err());
}

#[test]
fn a_before_scene_needs_a_second_valid_origin() {
    let parsed = Storyboard::parse(&both()).unwrap();
    for missing in [None, Some("")] {
        let error = Origins::select(&parsed, APP, missing).unwrap_err();
        assert!(error.contains("SHOWCASE_BEFORE_URL"), "{error}");
        assert!(error.contains("scene 2"), "{error}");
    }
    for invalid in [
        "https://127.0.0.1:8082/",
        "http://example.com:8082/",
        "http://127.0.0.1/",
        "http://user:pw@127.0.0.1:8082/",
        "http://127.0.0.1:8082/lab",
        "http://127.0.0.1:8082/?x=1",
        "http://127.0.0.1:8082/#x",
        "127.0.0.1:8082",
    ] {
        let error = Origins::select(&parsed, APP, Some(invalid)).unwrap_err();
        assert!(error.contains("SHOWCASE_BEFORE_URL="), "{invalid}: {error}");
        assert!(!error.contains("SHOWCASE_APP_URL"), "{invalid}: {error}");
    }
    // The same server twice films nothing new; the same port under another
    // loopback name could not be served twice inside the recorder either.
    for duplicate in [APP, "http://127.0.0.1:8080", "http://localhost:8080/"] {
        let error = Origins::select(&parsed, APP, Some(duplicate)).unwrap_err();
        assert!(error.contains("SHOWCASE_BEFORE_URL"), "{error}");
        assert!(error.contains("another port"), "{error}");
    }
    // A bad app origin is still named as such.
    let error = Origins::select(&parsed, "http://localhost:631/x", Some(BEFORE)).unwrap_err();
    assert!(error.contains("SHOWCASE_APP_URL="), "{error}");
    assert!(Origins::select(&parsed, APP, Some(BEFORE)).is_ok());
}

#[test]
fn only_the_builds_a_storyboard_films_are_bridged() {
    let after_only = Storyboard::parse(&board(&[scene("", "/")])).unwrap();
    // No before scene: the variable is ignored, even a bad one.
    let origins = Origins::select(&after_only, APP, Some("http://localhost:631/x")).unwrap();
    assert_eq!(
        origins.of(Build::After),
        Some(&AppOrigin::parse(APP).unwrap())
    );
    assert_eq!(origins.of(Build::Before), None);
    assert!(!origins.labelled());

    let before_only = Storyboard::parse(&board(&[scene(r#""build":"before","#, "/")])).unwrap();
    let origins = Origins::select(&before_only, APP, Some(BEFORE)).unwrap();
    assert_eq!(origins.of(Build::After), None);
    assert_eq!(origins.of(Build::Before).unwrap().http(), BEFORE);
    assert!(!origins.labelled());
    assert!(!origins.allows("http://127.0.0.1:8080/"));
    // Even unbridged, the app origin is validated and must differ.
    assert!(Origins::select(&before_only, APP, Some(APP)).is_err());
    assert!(Origins::select(&before_only, "http://x/", Some(BEFORE)).is_err());

    let cards =
        Storyboard::parse(r#"{"title":"x","scenes":[{"kind":"card","heading":"a"}]}"#).unwrap();
    let origins = Origins::select(&cards, "not a url", Some("neither")).unwrap();
    assert_eq!(origins, Origins::default());
    assert!(origins.bridged().is_empty());
}

#[test]
fn each_planned_scene_opens_its_own_builds_origin() {
    let parsed = Storyboard::parse(&both()).unwrap();
    let origins = Origins::select(&parsed, APP, Some(BEFORE)).unwrap();
    let planned = plan(&parsed, &[3500, 3000, 3000, 3000], &origins).unwrap();
    let json = serde_json::to_value(&planned).unwrap();
    assert_eq!(
        json["origins"],
        serde_json::json!([
            {"build": "after", "http": APP, "ws": "ws://127.0.0.1:8080/", "socket": "app.sock"},
            {"build": "before", "http": BEFORE, "ws": "ws://127.0.0.1:8082/", "socket": "before.sock"},
        ])
    );
    let scenes = &json["scenes"];
    assert_eq!(scenes[0].get("url"), None);
    assert_eq!(scenes[0].get("label"), None);
    assert_eq!(scenes[1]["build"], "before");
    assert_eq!(scenes[1]["url"], "http://127.0.0.1:8082/lab?seed=4");
    assert_eq!(scenes[1]["label"], "BEFORE (dev)");
    assert_eq!(scenes[2]["build"], "after");
    assert_eq!(scenes[2]["url"], "http://127.0.0.1:8080/lab?seed=4");
    assert_eq!(scenes[2]["label"], "AFTER (this PR)");
    assert_eq!(scenes[3]["build"], "after");
    assert_eq!(scenes[3]["url"], APP);
    // A storyboard filming one build needs no tag.
    let single = Storyboard::parse(&board(&[scene("", "/")])).unwrap();
    let origins = Origins::select(&single, APP, None).unwrap();
    let json = serde_json::to_value(plan(&single, &[3500, 3000], &origins).unwrap()).unwrap();
    assert_eq!(json["scenes"][1].get("label"), None);
    // A plan never films a build whose origin was not selected.
    let error = plan(&parsed, &[3500, 3000, 3000, 3000], &origins).unwrap_err();
    assert!(error.contains("before"), "{error}");
}

#[test]
fn the_filter_allows_exactly_the_configured_origins_and_one_per_scene() {
    let parsed = Storyboard::parse(&both()).unwrap();
    let origins = Origins::select(&parsed, APP, Some(BEFORE)).unwrap();
    assert!(origins.labelled());
    let (after, before) = (
        origins.of(Build::After).unwrap(),
        origins.of(Build::Before).unwrap(),
    );
    for (allowed, owner, other) in [
        ("http://127.0.0.1:8080/", after, before),
        (
            "http://127.0.0.1:8080/assets/client.wasm?v=2#x",
            after,
            before,
        ),
        ("ws://127.0.0.1:8080/ws", after, before),
        ("http://127.0.0.1:8082/", before, after),
        ("http://127.0.0.1:8082/lab", before, after),
        ("ws://127.0.0.1:8082/ws", before, after),
    ] {
        assert!(origins.allows(allowed), "refused {allowed}");
        // A scene reaches its own build only: never the other one.
        assert!(owner.allows(allowed), "scene refused {allowed}");
        assert!(!other.allows(allowed), "other scene allowed {allowed}");
    }
    for refused in [
        "http://127.0.0.1:8083/",
        "http://127.0.0.1:808/",
        "http://127.0.0.1:80820/",
        "http://localhost:8080/",
        "http://localhost:8082/",
        "http://localhost:631/",
        "http://127.0.0.2:8082/",
        "http://[::1]:8082/",
        "http://example.com:8082/",
        "https://127.0.0.1:8082/",
        "wss://127.0.0.1:8082/ws",
        "http://user@127.0.0.1:8082/",
        "http://127.0.0.1:8082.evil.test/",
        "ws://127.0.0.1:9000/ws",
        "file:///etc/passwd",
        "about:blank",
        "not a url",
    ] {
        assert!(!origins.allows(refused), "allowed {refused}");
    }
    assert_eq!(origins.bridged().len(), 2);
}

#[test]
fn check_refuses_a_before_scene_without_a_usable_before_url() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_env, temp) = Environment::setup();
    mode("SHOWCASE_LEVEL", "medium");
    std::fs::write(temp.path().join("storyboard.json"), both()).unwrap();
    let error = check(temp.path()).unwrap_err().to_string();
    assert!(error.contains("SHOWCASE_BEFORE_URL"), "{error}");
    for (value, why) in [
        ("", "SHOWCASE_BEFORE_URL"),
        ("http://localhost:631/admin", "SHOWCASE_BEFORE_URL="),
        (APP, "another port"),
    ] {
        mode("SHOWCASE_BEFORE_URL", value);
        let error = check(temp.path()).unwrap_err().to_string();
        assert!(error.contains(why), "{value}: {error}");
        assert!(make(temp.path()).is_err());
    }
    assert!(!temp.path().join("invocations.log").exists());
    mode("SHOWCASE_BEFORE_URL", BEFORE);
    check(temp.path()).unwrap();
    mode("SHOWCASE_APP_URL", BEFORE);
    assert!(check(temp.path()).is_err());
}

#[test]
fn a_before_and_after_take_bridges_both_origins_and_nothing_else() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_env, temp) = Environment::setup();
    mode("SHOWCASE_LEVEL", "medium");
    mode("SHOWCASE_BEFORE_URL", BEFORE);
    std::fs::write(
        temp.path().join("storyboard.json"),
        board(&[scene(r#""build":"before","#, "/lab"), scene("", "/lab")])
            .replace(r#"{"kind":"card","heading":"Opening"},"#, ""),
    )
    .unwrap();
    make(temp.path()).unwrap();
    let log = std::fs::read_to_string(temp.path().join("invocations.log")).unwrap();
    assert!(log.contains("--network none"), "{log}");
    assert!(!log.contains("--network host"), "{log}");
    assert!(log.contains("bridge socket app.sock"), "{log}");
    assert!(log.contains("bridge socket before.sock"), "{log}");
    assert!(
        log.contains(r#""url":"http://127.0.0.1:8082/lab","label":"BEFORE (dev)""#),
        "{log}"
    );
    assert!(
        log.contains(r#""url":"http://127.0.0.1:8080/lab","label":"AFTER (this PR)""#),
        "{log}"
    );
    assert!(log.contains(r#""socket":"before.sock""#), "{log}");

    mode("SHOWCASE_DOCKER_MODE", "fail");
    let error = make(temp.path()).unwrap_err().to_string();
    assert!(error.contains("http://127.0.0.1:8080/"), "{error}");
    assert!(error.contains("http://127.0.0.1:8082/"), "{error}");
}

#[test]
fn the_recorder_lets_each_scene_reach_only_its_own_build_and_tags_it() {
    let script = include_str!("../showcase/record.mjs");
    // The scene's origin is chosen before its page loads and dropped for
    // cards and terminals, so a before scene can never reach the PR's app.
    let chosen = script
        .find("current = origins.find((origin) => origin.build === scene.build) ?? null;")
        .unwrap();
    let goto = script.find("await page.goto(scene.url,").unwrap();
    assert!(chosen < goto);
    assert!(script.contains("current = null;"));
    assert_eq!(script.matches("current.http").count(), 1);
    assert_eq!(script.matches("current.ws").count(), 1);
    // No rule lets through "any configured origin".
    assert!(!script.contains("origins.some("));
    assert_eq!(script.matches("route.continue()").count(), 2);
    assert_eq!(script.matches("connectToServer").count(), 1);
    // The build tag is the plan's text, drawn with the caption on every
    // navigation and only when the plan gave the scene one.
    assert!(script.contains("document.getElementById(\"showcase-build\")?.remove();"));
    assert!(script.contains("if (label !== null) {"));
    assert!(script.contains("tag.textContent = label;"));
    assert!(!script.contains("BEFORE (dev)") && !script.contains("AFTER (this PR)"));
}

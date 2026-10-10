use super::{
    super::showcase::{AppOrigin, DEFAULT_APP_URL, Step, Storyboard, check, durations, make, plan},
    showcase_pipeline::{Environment, mode},
    showcase_security::ENV_LOCK,
};

const APP: &str = "http://127.0.0.1:8080/";

fn browser_board(path: &str, steps: &str) -> String {
    format!(
        r#"{{"title":"Lab","scenes":[
          {{"kind":"card","heading":"Opening"}},
          {{"kind":"browser","path":"{path}","caption":"The lab","seconds":3,"steps":[{steps}]}}
        ]}}"#
    )
}

#[test]
fn browser_scenes_plan_each_step_exactly_once() {
    let board = Storyboard::parse(&browser_board(
        "/lab",
        r##"{"click":"#start"},{"text":"abc"},{"key":"Enter"},{"wait_ms":2000}"##,
    ))
    .unwrap();
    // 3 s hold + 2000 ms wait + 3 characters typed at 22 ms; clicks and keys
    // are measured, not planned. The recorder holds what is left after them.
    assert_eq!(durations(&board, &[None, None])[1], 5066);
    // A voice stretches the whole scene, waits included, instead of adding
    // to it: 9000 + 600, not 9600 + 2000.
    assert_eq!(durations(&board, &[None, Some(9000)])[1], 9600);
    assert_eq!(durations(&board, &[None, Some(1000)])[1], 5066);
    assert_eq!(Step::WaitMs(7).planned_ms(), 7);
    assert_eq!(Step::Click("a".into()).planned_ms(), 0);
    assert_eq!(Step::Key("Enter".into()).planned_ms(), 0);
    assert_eq!(Step::Text("éé".into()).planned_ms(), 44);
}

#[test]
fn the_origin_filter_allows_only_the_app_under_development() {
    let app = AppOrigin::parse(APP).unwrap();
    for allowed in [
        "http://127.0.0.1:8080/",
        "http://127.0.0.1:8080",
        "http://127.0.0.1:8080/assets/client.wasm?v=2#x",
        "ws://127.0.0.1:8080/ws",
    ] {
        assert!(app.allows(allowed), "refused {allowed}");
    }
    for refused in [
        "http://127.0.0.1:8081/",
        "http://localhost:631/",
        "http://localhost:8080/",
        "http://127.0.0.2:8080/",
        "http://[::1]:8080/",
        "http://example.com/",
        "https://127.0.0.1:8080/",
        "http://user@127.0.0.1:8080/",
        "http://127.0.0.1:8080.evil.test/",
        "ws://127.0.0.1:9000/ws",
        "ws://example.com:8080/ws",
        "wss://127.0.0.1:8080/ws",
        "file:///etc/passwd",
        "about:blank",
        "not a url",
    ] {
        assert!(!app.allows(refused), "allowed {refused}");
    }
    assert_eq!(app.http(), APP);
    let localhost = AppOrigin::parse("http://localhost:5173").unwrap();
    assert!(localhost.allows("ws://localhost:5173/hmr"));
    assert!(!localhost.allows("http://127.0.0.1:5173/"));
}

#[test]
fn the_app_origin_must_be_a_plain_local_http_origin_with_a_port() {
    assert_eq!(DEFAULT_APP_URL, APP);
    for value in [
        "https://127.0.0.1:8080/",
        "http://example.com:8080/",
        "http://0.0.0.0:8080/",
        "http://[::1]:8080/",
        "http://127.0.0.1/",
        "http://127.0.0.1:80/",
        "http://user:pw@127.0.0.1:8080/",
        "http://127.0.0.1:8080/lab",
        "http://127.0.0.1:8080/?x=1",
        "http://127.0.0.1:8080/#x",
        "ws://127.0.0.1:8080/",
        "127.0.0.1:8080",
        "",
    ] {
        let error = AppOrigin::parse(value).unwrap_err();
        assert!(error.contains("SHOWCASE_APP_URL"), "{value}: {error}");
    }
}

#[test]
fn scene_paths_resolve_on_the_app_origin_only() {
    let app = AppOrigin::parse(APP).unwrap();
    assert_eq!(app.resolve("/").unwrap(), APP);
    assert_eq!(
        app.resolve("/lab?seed=4#map").unwrap(),
        "http://127.0.0.1:8080/lab?seed=4#map"
    );
    assert!(
        app.resolve("//example.com/")
            .unwrap_err()
            .contains("leaves")
    );
    assert!(
        app.resolve("http://localhost:631/")
            .unwrap_err()
            .contains("leaves")
    );
    assert!(
        app.resolve("ws://127.0.0.1:8080/ws")
            .unwrap_err()
            .contains("leaves")
    );
    assert!(app.resolve("http://[").is_err());
}

#[test]
fn browser_scenes_are_validated_with_the_storyboard() {
    let refuse = |board: String, why: &str| {
        let error = Storyboard::parse(&board).unwrap_err();
        assert!(error.contains(why), "{error}");
        assert!(error.contains("scene 2"), "{error}");
    };
    for path in [
        "lab",
        "//example.com/",
        "/a b",
        "/a\\\\b",
        "http://localhost:631/",
    ] {
        refuse(browser_board(path, ""), "browser path");
    }
    refuse(browser_board(&format!("/{}", "a".repeat(2048)), ""), "2048");
    for seconds in ["0", "301"] {
        refuse(
            browser_board("/", "").replace("\"seconds\":3", &format!("\"seconds\":{seconds}")),
            "1 to 300",
        );
    }
    let many = vec![r#"{"key":"a"}"#; 65].join(",");
    refuse(browser_board("/", &many), "limit is 64");
    refuse(browser_board("/", r#"{"click":" "}"#), "step 1: clicks");
    let long = format!(r#"{{"click":"{}"}}"#, "a".repeat(513));
    refuse(browser_board("/", &long), "selector");
    refuse(browser_board("/", r#"{"key":"Enter; rm"}"#), "key name");
    refuse(browser_board("/", r#"{"key":""}"#), "key name");
    let typed = format!(r#"{{"text":"{}"}}"#, "a".repeat(501));
    refuse(browser_board("/", &typed), "typed text");
    refuse(browser_board("/", r#"{"text":"a\u0007"}"#), "typed text");
    refuse(browser_board("/", r#"{"wait_ms":60001}"#), "60000 ms");

    let unknown = |board: String| assert!(Storyboard::parse(&board).is_err(), "{board}");
    unknown(browser_board("/", r#"{"scroll":1}"#));
    unknown(browser_board("/", r#"{"click":"a","key":"b"}"#));
    unknown(browser_board("/", "").replace("\"seconds\":3", "\"seconds\":3,\"url\":\"x\""));
    unknown(browser_board("/", "").replace("\"seconds\":3,", ""));

    let board = Storyboard::parse(&browser_board(
        "/lab",
        r#"{"click":"text=Start"},{"key":"Control+A"},{"text":""},{"wait_ms":60000}"#,
    ))
    .unwrap();
    assert!(board.scenes[1].narration().is_none());
}

#[test]
fn the_recorder_routes_http_and_websockets_through_the_same_origin_check() {
    let script = include_str!("../showcase/record.mjs");
    assert!(script.contains("const app = plan.app ?? null;"));
    assert!(script.contains("return new URL(value).href.startsWith(prefix);"));
    assert!(script.contains("context.routeWebSocket(/.*/"));
    assert!(script.contains("else await ws.close();"));
    assert!(script.contains("serviceWorkers: \"block\""));
    assert_eq!(script.matches("route.continue()").count(), 2);
    assert_eq!(script.matches("connectToServer").count(), 1);
    let http = script.find("context.route(\"**/*\"").unwrap();
    let websocket = script.find("context.routeWebSocket(").unwrap();
    let page = script.find("context.newPage()").unwrap();
    assert!(http < page && websocket < page);
}

#[test]
fn the_recorder_holds_after_loading_and_steps_but_measures_the_whole_scene() {
    let script = include_str!("../showcase/record.mjs");
    let at = |needle: &str| {
        script
            .find(needle)
            .unwrap_or_else(|| panic!("missing {needle}"))
    };
    let scene_start = at("const sceneStart = Date.now();");
    let load = at("if (scene.kind === \"browser\") await open(page, scene);");
    let begin = at("const begin = Date.now();");
    let steps = at("else await steps(page, scene);");
    let hold = at("const left = scene.duration_ms - (Date.now() - begin);");
    let measured = at("timings.scenes_ms.push(Date.now() - sceneStart);");
    assert!(scene_start < load && load < begin && begin < steps);
    assert!(steps < hold && hold < measured);
    // One hold per scene: steps are never waited for a second time.
    assert_eq!(script.matches("scene.duration_ms").count(), 1);
    assert_eq!(script.matches("sleep(step.wait_ms)").count(), 1);
    assert!(script.contains("page.keyboard.type(step.text, { delay: 22 })"));
}

#[test]
fn a_browser_take_has_no_network_but_the_bridged_app_origin() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_env, temp) = Environment::setup();
    mode("SHOWCASE_LEVEL", "medium");
    let board = temp.path().join("storyboard.json");
    std::fs::write(&board, browser_board("/lab", r#"{"wait_ms":10}"#)).unwrap();
    check(temp.path()).unwrap();
    make(temp.path()).unwrap();
    let log = std::fs::read_to_string(temp.path().join("invocations.log")).unwrap();
    // Replaces the former host-network assertions: the browser take has no
    // network, and the app's bridge socket exists while the recorder runs.
    assert!(log.contains("--network none"), "{log}");
    assert!(!log.contains("--network host"), "{log}");
    assert!(log.contains("bridge socket app.sock"), "{log}");
    assert!(log.contains(r#""app":{"http":"http://127.0.0.1:8080/","ws":"ws://127.0.0.1:8080/"}"#));
    assert!(log.contains(r#""url":"http://127.0.0.1:8080/lab""#));
    assert!(log.contains(r#""steps":[{"wait_ms":10}]"#));
    assert!(log.contains(r#""duration_ms":3010"#));

    let (_env, temp) = Environment::setup();
    mode("SHOWCASE_LEVEL", "medium");
    // A card and terminal take ignores the app setting and keeps no network.
    mode("SHOWCASE_APP_URL", "http://localhost:631/x");
    std::fs::write(
        temp.path().join("storyboard.json"),
        r#"{"title":"Cards","scenes":[{"kind":"card","heading":"a"},{"kind":"card","heading":"b"}]}"#,
    )
    .unwrap();
    make(temp.path()).unwrap();
    let log = std::fs::read_to_string(temp.path().join("invocations.log")).unwrap();
    assert!(log.contains("--network none"), "{log}");
    assert!(!log.contains("--network host"), "{log}");
    assert!(!log.contains("bridge socket"), "{log}");
    assert!(log.contains(r#""app":null"#));
}

#[test]
fn a_bad_app_origin_is_refused_by_the_check_before_any_side_effect() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_env, temp) = Environment::setup();
    mode("SHOWCASE_LEVEL", "medium");
    std::fs::write(temp.path().join("storyboard.json"), browser_board("/", "")).unwrap();
    mode("SHOWCASE_APP_URL", "http://localhost:631/admin");
    let error = check(temp.path()).unwrap_err().to_string();
    assert!(error.contains("SHOWCASE_APP_URL"), "{error}");
    assert!(make(temp.path()).is_err());
    assert!(!temp.path().join("invocations.log").exists());

    mode("SHOWCASE_APP_URL", "http://localhost:5173");
    check(temp.path()).unwrap();
    mode("SHOWCASE_DOCKER_MODE", "fail");
    let error = make(temp.path()).unwrap_err().to_string();
    assert!(
        error.contains("is the app up at http://localhost:5173/"),
        "{error}"
    );
}

#[test]
fn the_plan_gives_browser_scenes_their_absolute_url_and_needs_the_origin() {
    let board = Storyboard::parse(&browser_board("/lab", "")).unwrap();
    let app = AppOrigin::parse(APP).unwrap();
    let planned = plan(&board, &[3500, 3000], Some(app)).unwrap();
    let json = serde_json::to_value(&planned).unwrap();
    assert_eq!(json["app"]["ws"], "ws://127.0.0.1:8080/");
    assert_eq!(json["scenes"][0].get("url"), None);
    assert_eq!(json["scenes"][1]["url"], "http://127.0.0.1:8080/lab");
    assert_eq!(json["scenes"][1]["duration_ms"], 3000);
    assert!(
        plan(&board, &[3500, 3000], None)
            .unwrap_err()
            .contains("need the app origin")
    );
}

#[test]
fn a_storyboard_may_ask_to_film_the_app_on_webgpu() {
    let default = Storyboard::parse(&browser_board("/", "")).unwrap();
    let app = AppOrigin::parse(APP).unwrap();
    let json = serde_json::to_value(plan(&default, &[3500, 3000], Some(app)).unwrap()).unwrap();
    assert_eq!(json["renderer"], "default");

    let text = browser_board("/", "").replacen('{', r#"{"renderer":"webgpu","#, 1);
    let webgpu = Storyboard::parse(&text).unwrap();
    let app = AppOrigin::parse(APP).unwrap();
    let json = serde_json::to_value(plan(&webgpu, &[3500, 3000], Some(app)).unwrap()).unwrap();
    assert_eq!(json["renderer"], "webgpu");

    let unknown = browser_board("/", "").replacen('{', r#"{"renderer":"vulkan","#, 1);
    assert!(Storyboard::parse(&unknown).is_err());
}

use super::parse;
use crate::ship::compare::{
    bookmarks::Bookmark,
    capture::{fragment, refusal},
    report::{Camera, Capture},
    server,
    settle::Sample,
};

const HASH: &str = "ab12";

fn bookmark(renderer: &str) -> Bookmark {
    let text = format!(
        r#"{{"name": "alps", "map": "france", "x": 17109, "y": 9921, "zoom": 1, "renderer": "{renderer}"}}"#
    );
    parse(&[text]).expect("file").bookmarks.remove(0)
}

fn settled(renderer: &str, map_hash: &str) -> Capture {
    Capture {
        camera: Some(Camera {
            x: 17109.0,
            y: 9921.0,
            zoom: 1.0,
        }),
        history: vec![
            Sample {
                at: 0,
                pending: 0,
                failed: 0,
                frame: 9,
            },
            Sample {
                at: 1500,
                pending: 0,
                failed: 0,
                frame: 9,
            },
        ],
        renderer: Some(renderer.into()),
        map_hash: Some(map_hash.into()),
        ..Capture::default()
    }
}

#[test]
fn the_url_is_the_bookmark_as_a_camera_fragment() {
    assert_eq!(
        fragment(&bookmark("webgl2")),
        "x=17109&y=9921&zoom=1&renderer=webgl2"
    );
    let fractional = parse(&[
        r#"{"name": "a", "map": "france", "x": 252.5, "y": 0.25, "zoom": 0.698}"#.to_owned(),
    ])
    .expect("file");
    assert_eq!(
        fragment(&fractional.bookmarks[0]),
        "x=252.5&y=0.25&zoom=0.698&renderer=webgpu"
    );
}

#[test]
fn a_settled_matching_view_is_accepted() {
    assert_eq!(
        refusal(&bookmark("webgl2"), HASH, &settled("webgl2", HASH)),
        None
    );
}

#[test]
fn a_renderer_that_differs_from_the_bookmark_is_refused_naming_both() {
    let reason = refusal(&bookmark("webgl2"), HASH, &settled("webgpu", HASH)).expect("refused");
    assert!(
        reason.contains("webgl2") && reason.contains("webgpu"),
        "{reason}"
    );
}

#[test]
fn a_map_other_than_the_package_asked_for_is_refused() {
    let reason = refusal(&bookmark("webgpu"), HASH, &settled("webgpu", "cd34")).expect("refused");
    assert!(reason.contains("cd34") && reason.contains(HASH), "{reason}");
}

#[test]
fn failed_chunks_are_refused_with_their_count() {
    // Three chunks keep failing for 12 s; the page counts them as pending too.
    let mut capture = settled("webgpu", HASH);
    capture.history = (0..=49)
        .map(|i| {
            let missing = if i == 0 { 0 } else { 3 };
            Sample {
                at: i * 250,
                pending: missing,
                failed: missing,
                frame: 9,
            }
        })
        .collect();
    let reason = refusal(&bookmark("webgpu"), HASH, &capture).expect("refused");
    assert!(reason.contains("3 chunks failed"), "{reason}");
}

#[test]
fn a_view_that_did_not_settle_or_did_not_land_is_refused() {
    let mut pending = settled("webgpu", HASH);
    pending.history = vec![Sample {
        at: 0,
        pending: 4,
        failed: 0,
        frame: 3,
    }];
    let reason = refusal(&bookmark("webgpu"), HASH, &pending).expect("refused");
    assert!(reason.contains("not settled"), "{reason}");

    let mut elsewhere = settled("webgpu", HASH);
    elsewhere.camera = Some(Camera {
        x: 17110.0,
        y: 9921.0,
        zoom: 1.0,
    });
    let reason = refusal(&bookmark("webgpu"), HASH, &elsewhere).expect("refused");
    assert!(reason.contains("17110"), "{reason}");

    let broken = Capture {
        error: Some("the page never loaded".into()),
        ..Capture::default()
    };
    assert_eq!(
        refusal(&bookmark("webgpu"), HASH, &broken).as_deref(),
        Some("the page never loaded")
    );
}

#[test]
fn the_side_environment_opens_the_package_at_server_start() {
    let env = server::side_env("pack", std::path::Path::new("/maps"), HASH);
    assert!(
        env.contains(&("AOE_OPEN_MAP_PACKAGE", HASH.to_owned())),
        "{env:?}"
    );
    assert!(
        env.contains(&("AOE_MAP_PACKAGE_DIRECTORY", "/maps".to_owned())),
        "{env:?}"
    );
}

#[test]
fn the_side_environment_is_exactly_the_spectator_setup() {
    // No controller or lease setting: one added to the server's environment has
    // to be named here on purpose, and that review is what this guards.
    let env = server::side_env("pack", std::path::Path::new("/maps"), HASH);
    let names: Vec<&str> = env.iter().map(|(name, _)| *name).collect();
    assert_eq!(
        names,
        [
            "AOE_ASSET_PACK",
            "AOE_MAP_PACKAGE_DIRECTORY",
            "AOE_OPEN_MAP_PACKAGE"
        ]
    );
}

#[test]
fn the_capture_page_watches_the_app_and_never_acts_on_it() {
    // The page is a spectator: it loads, evaluates, screenshots and reads its
    // errors. Any other page method (click, mouse, keyboard, ...) or a dispatched
    // input event would act on the app as a user, so the script may not contain one.
    const WATCHING: [&str; 5] = ["evaluate", "goto", "waitForFunction", "on", "screenshot"];
    const ACTING: [&str; 6] = [
        ".click(",
        "dispatchEvent",
        "mouse.",
        "keyboard.",
        "WheelEvent",
        ".wheel(",
    ];
    let script = include_str!("../capture.mjs");
    let calls: Vec<String> = script
        .split("page.")
        .skip(1)
        .map(|rest| {
            rest.chars()
                .take_while(|c| c.is_ascii_alphabetic())
                .collect()
        })
        .collect();
    assert!(!calls.is_empty(), "the script calls the page");
    for call in &calls {
        assert!(
            WATCHING.contains(&call.as_str()),
            "page.{call} in capture.mjs"
        );
    }
    for act in ACTING {
        assert!(!script.contains(act), "{act} in capture.mjs");
    }
}

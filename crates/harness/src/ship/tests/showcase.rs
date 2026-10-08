use super::super::describe::Level;
use super::super::showcase::{
    Scene, Segment, Storyboard, Style, Timings, Tone, check_duration, durations, measured_duration,
    resolve_out, track,
};

const BOARD: &str = r#"{
  "title": "make ship",
  "scenes": [
    {"kind": "card", "heading": "Before and after", "lines": ["one", "two"]},
    {"kind": "terminal", "tag": "dev judge", "tone": "before", "caption": "An agent pushes",
     "lines": [{"style": "cmd", "text": "git push"}, {"style": "good", "text": "allowed"}, {"style": "out", "text": ""}],
     "narration": "Before this change, any agent could push."}
  ]
}"#;

#[test]
fn a_storyboard_parses_with_defaults() {
    let board = Storyboard::parse(BOARD).unwrap();
    assert_eq!(board.voice, "Kore");
    assert_eq!(board.scenes.len(), 2);
    let Scene::Terminal { tone, lines, .. } = &board.scenes[1] else {
        panic!("a terminal scene");
    };
    assert_eq!(*tone, Tone::Before);
    assert_eq!(lines[0].style, Style::Cmd);
    assert_eq!(board.scenes[0].narration(), None);
    assert!(board.scenes[1].narration().is_some());
    assert!(
        Storyboard::parse(&BOARD.replace("\"kind\": \"terminal\"", "\"kind\": \"video\""))
            .unwrap_err()
            .contains("unknown variant")
    );
}

#[test]
fn storyboards_are_refused_when_they_break_a_limit() {
    let refuse = |board: &str, why: &str| {
        let error = Storyboard::parse(board).unwrap_err();
        assert!(error.contains(why), "{error}");
    };
    refuse(r#"{"title": "x", "scenes": []}"#, "no scenes");
    refuse(
        &BOARD.replace("\"tone\": \"before\"", "\"tone\": \"before\", \"zoom\": 2"),
        "zoom",
    );
    refuse(
        &BOARD.replace("\"style\": \"cmd\"", "\"style\": \"bold\""),
        "bold",
    );
    refuse(
        &BOARD.replace(
            "\"title\": \"make ship\"",
            "\"title\": \"x\", \"voice\": \"Kore; rm\"",
        ),
        "voice",
    );
    let long = "a".repeat(3001);
    refuse(
        &BOARD.replace("Before this change, any agent could push.", &long),
        "3000",
    );
}

#[test]
fn scenes_last_long_enough_for_their_voice() {
    let board = Storyboard::parse(BOARD).unwrap();
    // card 3500 + 2×400; terminal: "git push" typed (8×22 + 500), one line
    // shown (650), then 3000 held.
    assert_eq!(durations(&board, &[None, None]), [4300, 4526]);
    assert_eq!(durations(&board, &[None, Some(9000)]), [4300, 9600]);
    assert_eq!(durations(&board, &[None, Some(1000)])[1], 4526);
}

#[test]
fn planned_and_measured_duration_checks_enforce_five_minutes() {
    assert!(check_duration(Level::Max, &[299_000, 1_000]).is_ok());
    assert!(check_duration(Level::Max, &[300_001]).is_err());
    let at_limit = Timings {
        lead_in_ms: 1_000,
        scenes_ms: vec![299_000],
    };
    assert_eq!(
        measured_duration(Level::Max, &at_limit, 1).unwrap(),
        300_000
    );
    let over_limit = Timings {
        lead_in_ms: 1_000,
        scenes_ms: vec![299_001],
    };
    assert!(
        measured_duration(Level::Max, &over_limit, 1)
            .unwrap_err()
            .contains("recorded showcase runs 300.0s")
    );
    assert!(
        measured_duration(Level::Max, &at_limit, 2)
            .unwrap_err()
            .contains("measured 1 scene(s)")
    );
}

#[test]
fn narration_stretch_can_put_the_planned_showcase_over_five_minutes() {
    let board = Storyboard::parse(
        r#"{"title":"long","scenes":[
          {"kind":"card","heading":"one","narration":"first"},
          {"kind":"card","heading":"two","narration":"second"}
        ]}"#,
    )
    .unwrap();
    let planned = durations(&board, &[Some(150_000), Some(150_000)]);
    assert_eq!(planned, [150_600, 150_600]);
    assert!(check_duration(Level::Max, &planned).is_err());
}

#[test]
fn showcase_duration_uses_the_selected_shipping_level_limit() {
    let board = Storyboard::parse(&format!(
        r#"{{"title":"medium limit","scenes":[{{"kind":"card","heading":"long","lines":[{}]}}]}}"#,
        vec!["\"line\""; 144].join(",")
    ))
    .unwrap();
    let planned = durations(&board, &[None]);
    assert_eq!(planned, [61_100]);
    assert!(check_duration(Level::Medium, &planned).is_err());
    assert!(check_duration(Level::High, &planned).is_ok());
}

#[test]
fn showcase_container_images_accept_only_the_documented_name_characters() {
    use super::super::showcase::validate_image_value;

    assert!(validate_image_value("aoeworld/browser:5.1.9").is_ok());
    for invalid in ["", "x; touch /tmp/pwned #", "Upper/Image", "image name"] {
        assert!(
            validate_image_value(invalid).is_err(),
            "accepted {invalid:?}"
        );
    }
}

#[test]
fn showcase_output_is_created_and_confined_to_its_cache_directory() {
    let root = tempfile::tempdir().unwrap();
    let output = resolve_out(root.path(), ".cache/showcase/nested/take.webm").unwrap();
    assert!(output.path.starts_with(root.path().join(".cache/showcase")));
    assert!(output.path.parent().unwrap().is_dir());
    assert!(resolve_out(root.path(), ".git/config").is_err());
    assert!(resolve_out(root.path(), "video.webm").is_err());
    assert!(resolve_out(root.path(), ".cache/showcase/../escape.webm").is_err());
    assert!(resolve_out(root.path(), ".cache/showcase/take.mp4").is_err());
    assert!(resolve_out(root.path(), ".cache/showcase/take.WEBM").is_err());
}

#[test]
fn showcase_output_rejects_symlinks_that_escape_the_cache_directory() {
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let showcase = root.path().join(".cache/showcase");
    std::fs::create_dir_all(&showcase).unwrap();
    symlink(outside.path(), showcase.join("escape")).unwrap();
    assert!(resolve_out(root.path(), ".cache/showcase/escape/video.webm").is_err());

    let target = outside.path().join("protected.webm");
    std::fs::write(&target, b"protected").unwrap();
    symlink(&target, showcase.join("video.webm")).unwrap();
    assert!(resolve_out(root.path(), ".cache/showcase/video.webm").is_err());
}

#[test]
fn showcase_output_rejects_symlinks_in_cache_roots_and_nested_components() {
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join(".git")).unwrap();
    symlink(root.path().join(".git"), root.path().join(".cache")).unwrap();
    assert!(resolve_out(root.path(), ".cache/showcase/video.webm").is_err());

    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join(".cache")).unwrap();
    symlink(
        root.path().join(".cache"),
        root.path().join(".cache/showcase"),
    )
    .unwrap();
    assert!(resolve_out(root.path(), ".cache/showcase/video.webm").is_err());

    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join(".cache/showcase")).unwrap();
    std::fs::create_dir(root.path().join("elsewhere")).unwrap();
    symlink(
        root.path().join("elsewhere"),
        root.path().join(".cache/showcase/nested"),
    )
    .unwrap();
    assert!(resolve_out(root.path(), ".cache/showcase/nested/video.webm").is_err());
}

#[test]
fn showcase_output_rejects_existing_non_regular_files() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join(".cache/showcase/video.webm");
    std::fs::create_dir_all(&output).unwrap();
    assert!(resolve_out(root.path(), ".cache/showcase/video.webm").is_err());
}

#[test]
fn showcase_output_rejects_hard_links() {
    let root = tempfile::tempdir().unwrap();
    let showcase = root.path().join(".cache/showcase");
    std::fs::create_dir_all(&showcase).unwrap();
    let protected = root.path().join("protected.json");
    let output = showcase.join("showcase.webm");
    std::fs::write(&protected, b"protected registry").unwrap();
    std::fs::hard_link(&protected, &output).unwrap();
    assert!(resolve_out(root.path(), ".cache/showcase/showcase.webm").is_err());
    assert_eq!(std::fs::read(protected).unwrap(), b"protected registry");
}

#[test]
fn atomic_showcase_publish_replaces_a_hard_link_without_writing_through_it() {
    use super::super::showcase::publish_temp_output;

    let root = tempfile::tempdir().unwrap();
    let showcase = root.path().join(".cache/showcase");
    std::fs::create_dir_all(&showcase).unwrap();
    let protected = root.path().join("protected.json");
    let output = resolve_out(root.path(), ".cache/showcase/showcase.webm").unwrap();
    let output_path = showcase.join("showcase.webm");
    std::fs::write(&protected, b"protected registry").unwrap();
    std::fs::hard_link(&protected, &output_path).unwrap();
    let source = root.path().join("encoded.webm");
    std::fs::write(&source, b"new video").unwrap();

    let work = tempfile::tempdir().unwrap();
    publish_temp_output(&source, &output, work.path()).unwrap();

    assert_eq!(std::fs::read(protected).unwrap(), b"protected registry");
    assert_eq!(std::fs::read(output_path).unwrap(), b"new video");
    // No temporary file is left next to the published video.
    let left: Vec<_> = std::fs::read_dir(&showcase)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(left, vec![std::ffi::OsString::from("showcase.webm")]);
}

#[cfg(unix)]
#[test]
fn showcase_publish_uses_the_validated_directory_fd_after_path_swap() {
    use super::super::showcase::publish_temp_output;
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().unwrap();
    let output = resolve_out(root.path(), ".cache/showcase/take.webm").unwrap();
    let source = root.path().join("encoded.webm");
    std::fs::write(&source, b"showcase bytes").unwrap();
    std::fs::write(root.path().join("Cargo.toml"), b"repository manifest").unwrap();

    let showcase = root.path().join(".cache/showcase");
    let original = root.path().join(".cache/showcase-original");
    std::fs::rename(&showcase, &original).unwrap();
    symlink(root.path(), &showcase).unwrap();

    let work = tempfile::tempdir().unwrap();
    publish_temp_output(&source, &output, work.path()).unwrap();

    assert_eq!(
        std::fs::read(root.path().join("Cargo.toml")).unwrap(),
        b"repository manifest"
    );
    assert_eq!(
        std::fs::read(original.join("take.webm")).unwrap(),
        b"showcase bytes"
    );
}

#[test]
fn recorder_aborts_every_request_except_inline_document_urls() {
    let script = include_str!("../showcase/record.mjs");
    assert!(script.contains("serviceWorkers: \"block\""));
    assert!(script.contains("context.route(\"**/*\""));
    assert!(script.contains("route.abort(\"blockedbyclient\")"));
    assert!(script.contains("url === \"about:blank\" || url.startsWith(\"data:\")"));
    // Browser scenes (showcase_browser.rs) add exactly one other origin, the
    // app's, for HTTP and WebSockets alike; nothing else is ever navigated to.
    assert!(
        script.contains("else if (app !== null && within(url, app.http)) await route.continue();")
    );
    assert!(script.contains("if (app !== null && within(ws.url(), app.ws)) ws.connectToServer();"));
    assert!(!script.contains("browserScene"));
    assert_eq!(script.matches("page.goto(").count(), 2);
    assert!(script.contains("page.goto(scene.url,"));
    assert!(script.contains("page.goto(\"about:blank\")"));
}

#[test]
fn recorder_runs_without_container_network() {
    // Only a browser scene gives the recorder the host network (filtered to
    // the app origin); the take itself is checked in showcase_browser.rs.
    let media = include_str!("../showcase/media.rs");
    assert!(media.contains("let network = if plan.app.is_some() { \"host\" } else { \"none\" };"));
    assert!(media.contains("\"--network\",\n            network,"));
    assert_eq!(media.matches("\"--network\"").count(), 1);
}

#[test]
fn showcase_target_checks_before_building_its_docker_images() {
    let makefile = include_str!("../../../../../make/ship.mk");
    let target = makefile.find("showcase: showcase-check").unwrap();
    let execute = makefile.find("harness.sh exec showcase\n").unwrap();
    assert!(target < execute);
    assert!(!makefile.contains("$(MAKE) --no-print-directory ship-tools browser-deps"));
    assert!(makefile.contains("export MAKE BROWSER_IMAGE SHIP_TOOLS_IMAGE"));
    assert!(!makefile.contains("$(BROWSER_IMAGE)"));
    assert!(!makefile.contains("$(SHIP_TOOLS_IMAGE)"));
    assert!(makefile.contains("harness.sh exec showcase-check"));
    let hook = include_str!("../../../../../.agents/hooks/harness.sh");
    assert!(hook.contains("showcase-check) probe=crates/harness/src/ship/showcase/check.rs"));

    let media = include_str!("../showcase/media.rs");
    assert!(media.contains("pub(crate) fn check(root: &Path)"));
    assert!(media.contains("let (board, out, _) = inputs(root)?;"));
    let check = media.find("pub(crate) fn check(root: &Path)").unwrap();
    let make = media.find("pub(crate) fn make(root: &Path)").unwrap();
    assert!(check < make);
}

#[test]
fn ffmpeg_voice_and_segment_conversions_are_batched() {
    let media = include_str!("../showcase/media.rs");
    let narrate = media.find("fn narrate(").unwrap();
    let mix = media.find("fn mix(").unwrap();
    let inputs = media.find("fn inputs(").unwrap();
    let narration_source = &media[narrate..mix];
    let segments_source = &media[mix..inputs];
    assert!(!narration_source.contains("docker("));
    assert_eq!(
        segments_source
            .matches("docker(&[work], tools, &args)")
            .count(),
        1
    );
    let segment_loop = segments_source.find("for (index, segment)").unwrap();
    let segment_batch = segments_source
        .find("docker(&[work], tools, &args)")
        .unwrap();
    assert!(segment_loop < segment_batch);
    assert!(segments_source.contains("voice-{v}.pcm"));
    assert!(segments_source.contains("super::PCM_RATE.to_string()"));
}

#[test]
fn concat_manifest_escapes_apostrophes_in_segment_paths() {
    use super::super::showcase::concat_entry;
    use std::path::Path;

    assert_eq!(
        concat_entry(Path::new("/tmp/O'Brien/AoeWorld/.cache/tmp/segment-0.wav")),
        "file '/tmp/O'\\''Brien/AoeWorld/.cache/tmp/segment-0.wav'\n"
    );
}

#[test]
fn tts_command_reads_authorization_from_stdin_and_bounds_bodies() {
    use super::super::showcase::tts_command;
    use std::path::Path;

    let command = tts_command(Path::new("request.json"));
    let args: Vec<String> = command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    assert_eq!(args.first().map(String::as_str), Some("-q"));
    assert!(args.windows(2).any(|pair| pair == ["-H", "@-"]));
    assert!(!args.iter().any(|arg| arg == "--max-filesize"));
    assert!(!args.iter().any(|arg| arg == "33554432"));
    assert!(args.contains(&"--fail-with-body".into()));
    assert!(!args.iter().any(|arg| arg.contains("DUMMY_OPENROUTER_KEY")));
    let source = include_str!("../showcase/media.rs");
    assert!(source.contains("command.stdout(Stdio::piped())"));
    assert!(source.contains("take(limit + 1)"));
    assert!(!source.contains("work.join(\"headers\")"));
    assert!(source.contains("file.take(TTS_ERROR_LIMIT).read_to_string"));
}

#[cfg(unix)]
#[test]
fn tts_stream_limit_stops_a_stubbed_unknown_length_body() {
    use super::super::showcase::save_tts_response;
    use std::process::{Command, Stdio};

    let root = tempfile::tempdir().unwrap();
    let audio = root.path().join("response.pcm");
    let mut child = Command::new("sh")
        .args(["-c", "head -c 33554433 /dev/zero"])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let error = save_tts_response(&mut child, &audio).unwrap_err();
    assert!(error.to_string().contains("33554432-byte limit"));
    assert_eq!(std::fs::metadata(audio).unwrap().len(), 32 * 1024 * 1024);
}

#[test]
fn showcase_preflights_both_plans_before_starting_the_recorder() {
    let source = include_str!("../showcase/media.rs");
    let preflight = source.find("check_duration(level, &durations(").unwrap();
    let narration = source.find("narrate(work_path, &board, level)").unwrap();
    let stretched_check = narration
        + source[narration..]
            .find("check_duration(level, &planned)")
            .unwrap();
    let recording = source.find("record(root, work_path, &plan)").unwrap();
    assert!(preflight < narration);
    assert!(narration < stretched_check);
    assert!(stretched_check < recording);
    let build = source.find("ship-tools\", \"browser-deps").unwrap();
    assert!(stretched_check < build);
    assert!(build < recording);
    assert!(
        source.contains("const TTS_URL: &str = \"https://openrouter.ai/api/v1/audio/speech\";")
    );
    assert!(!source.contains("AOE_TTS_URL"));
    let docs = include_str!("../../../../../docs/showcase.md");
    assert!(!docs.contains("AOE_TTS_URL"));
}

#[test]
fn the_narration_track_follows_the_measured_video() {
    let timings = Timings {
        lead_in_ms: 420,
        scenes_ms: vec![4300, 9700, 6100],
    };
    let voices = [None, Some(9000), None];
    assert_eq!(
        track(&timings, &voices).unwrap(),
        [
            Segment {
                voice: None,
                ms: 420
            },
            Segment {
                voice: None,
                ms: 4300
            },
            Segment {
                voice: Some(1),
                ms: 9700
            },
            Segment {
                voice: None,
                ms: 6100
            },
        ]
    );
    let no_lead_in = Timings {
        lead_in_ms: 0,
        ..timings.clone()
    };
    assert_eq!(track(&no_lead_in, &voices).unwrap().len(), 3);
    assert!(track(&timings, &[None]).unwrap_err().contains("measured 3"));
}

#[test]
fn speech_length_comes_from_the_pcm_size() {
    use super::super::showcase::pcm_seconds;
    // A real 3.44 s answer from Gemini TTS: 24 kHz, 16-bit, mono.
    assert!((pcm_seconds(165_120) - 3.44).abs() < 1e-9);
    assert_eq!(pcm_seconds(0), 0.0);
}

#[test]
fn storyboard_reads_are_bounded_and_require_regular_files() {
    use super::super::showcase::read_storyboard;

    let oversized = tempfile::NamedTempFile::new().unwrap();
    use std::io::Write as _;
    let mut file = oversized.reopen().unwrap();
    file.write_all(&vec![b' '; 256 * 1024 + 1]).unwrap();
    assert!(
        read_storyboard(oversized.path())
            .unwrap_err()
            .to_string()
            .contains("262144")
    );

    #[cfg(unix)]
    assert!(read_storyboard(std::path::Path::new("/dev/zero")).is_err());

    let directory = tempfile::tempdir().unwrap();
    assert!(read_storyboard(directory.path()).is_err());
}

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
        Storyboard::parse(&BOARD.replace("\"kind\": \"terminal\"", "\"kind\": \"browser\""))
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
    assert!(check_duration(&[299_000, 1_000]).is_ok());
    assert!(check_duration(&[300_001]).is_err());
    let at_limit = Timings {
        lead_in_ms: 1_000,
        scenes_ms: vec![299_000],
    };
    assert_eq!(measured_duration(&at_limit, 1).unwrap(), 300_000);
    let over_limit = Timings {
        lead_in_ms: 1_000,
        scenes_ms: vec![299_001],
    };
    assert!(
        measured_duration(&over_limit, 1)
            .unwrap_err()
            .contains("recorded showcase runs 300s")
    );
    assert!(
        measured_duration(&at_limit, 2)
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
    assert!(check_duration(&planned).is_err());
}

#[test]
fn showcase_output_is_created_and_confined_to_its_cache_directory() {
    let root = tempfile::tempdir().unwrap();
    let output = resolve_out(root.path(), ".cache/showcase/nested/take.webm").unwrap();
    assert!(output.starts_with(root.path().join(".cache/showcase")));
    assert!(output.parent().unwrap().is_dir());
    assert!(resolve_out(root.path(), ".git/config").is_err());
    assert!(resolve_out(root.path(), "video.webm").is_err());
    assert!(resolve_out(root.path(), ".cache/showcase/../escape.webm").is_err());
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
    let output = showcase.join("showcase.webm");
    let temp = showcase.join(".showcase.tmp.webm");
    std::fs::write(&protected, b"protected registry").unwrap();
    std::fs::hard_link(&protected, &output).unwrap();
    std::fs::write(&temp, b"new video").unwrap();

    publish_temp_output(&temp, &output).unwrap();

    assert_eq!(std::fs::read(protected).unwrap(), b"protected registry");
    assert_eq!(std::fs::read(output).unwrap(), b"new video");
    assert!(!temp.exists());
}

#[test]
fn recorder_aborts_every_request_except_inline_document_urls() {
    let script = include_str!("../showcase/record.mjs");
    assert!(script.contains("serviceWorkers: \"block\""));
    assert!(script.contains("context.route(\"**/*\""));
    assert!(script.contains("route.abort(\"blockedbyclient\")"));
    assert!(script.contains("url === \"about:blank\" || url.startsWith(\"data:\")"));
    assert!(!script.contains("routeWebSocket"));
    assert!(!script.contains("browserScene"));
    assert!(!script.contains("page.goto"));
}

#[test]
fn recorder_runs_without_container_network() {
    let media = include_str!("../showcase/media.rs");
    assert!(media.contains("\"--network\",\n            \"none\""));
    assert!(!media.contains("\"--network\",\n            \"host\""));
}

#[test]
fn showcase_target_checks_before_building_its_docker_images() {
    let makefile = include_str!("../../../../../make/ship.mk");
    let target = makefile.find("showcase: showcase-check").unwrap();
    let execute = makefile.find("harness.sh exec showcase\n").unwrap();
    assert!(target < execute);
    assert!(!makefile.contains("$(MAKE) --no-print-directory ship-tools browser-deps"));
    assert!(makefile.contains("export MAKE BROWSER_IMAGE SHIP_TOOLS_IMAGE"));
    assert!(makefile.contains("harness.sh exec showcase-check"));
    let hook = include_str!("../../../../../.agents/hooks/harness.sh");
    assert!(hook.contains("showcase-check) probe=crates/harness/src/ship/showcase/check.rs"));

    let media = include_str!("../showcase/media.rs");
    assert!(media.contains("pub(crate) fn check(root: &Path)"));
    assert!(media.contains("let (board, out) = inputs(root)?;"));
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

    let command = tts_command(Path::new("request.json"), Path::new("response.pcm"));
    let args: Vec<String> = command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    assert!(args.windows(2).any(|pair| pair == ["-H", "@-"]));
    assert!(
        args.windows(2)
            .any(|pair| pair == ["--max-filesize", "33554432"])
    );
    assert!(args.contains(&"--fail-with-body".into()));
    assert!(!args.iter().any(|arg| arg.contains("DUMMY_OPENROUTER_KEY")));
    let source = include_str!("../showcase/media.rs");
    assert!(!source.contains("work.join(\"headers\")"));
    assert!(source.contains("file.take(TTS_ERROR_LIMIT).read_to_string"));
}

#[test]
fn showcase_preflights_both_plans_before_starting_the_recorder() {
    let source = include_str!("../showcase/media.rs");
    let preflight = source.find("check_duration(&durations(").unwrap();
    let narration = source.find("narrate(&work, &board)").unwrap();
    let stretched_check = source.find("check_duration(&planned)").unwrap();
    let recording = source.find("record(root, &work, &plan)").unwrap();
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

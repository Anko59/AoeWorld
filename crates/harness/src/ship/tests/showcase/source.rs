#[test]
fn ffmpeg_voice_and_segment_conversions_are_batched() {
    let media = include_str!("../../showcase/media.rs");
    let mix = media.find("fn mix(").unwrap();
    let inputs = media.find("fn inputs(").unwrap();
    let narration_source = include_str!("../../showcase/narration/mod.rs");
    let segments_source = &media[mix..inputs];
    assert!(!narration_source.contains("docker("));
    let record = media.find("fn record(").unwrap();
    assert!(!media[record..mix].contains("docker("));
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
fn tts_command_reads_authorization_from_stdin_and_bounds_bodies() {
    use crate::ship::showcase::tts_command;
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
    let source = include_str!("../../showcase/narration/mod.rs");
    assert!(source.contains("command.stdout(Stdio::piped())"));
    assert!(source.contains("take(limit + 1)"));
    assert!(!source.contains("work.join(\"headers\")"));
    let media = include_str!("../../showcase/media.rs");
    let cache = include_str!("../../showcase/narration/cache.rs");
    assert!(!media.contains("work.join(\"headers\")"));
    assert!(!cache.contains("work.join(\"headers\")"));
    assert!(source.contains("file.take(TTS_ERROR_LIMIT).read_to_string"));
}

#[test]
fn showcase_preflights_both_plans_before_starting_the_recorder() {
    let source = include_str!("../../showcase/media.rs");
    let preflight = source.find("check_duration(level, &planned)").unwrap();
    let narration = source
        .find("narrate(root, work_path, &board, level)")
        .unwrap();
    let stretched_check = narration
        + source[narration..]
            .find("check_duration(level, &planned)")
            .unwrap();
    let recording = source.find("record(root, work_path, &plan)").unwrap();
    assert!(preflight < narration);
    assert!(narration < stretched_check);
    assert!(stretched_check < recording);
    let build = source
        .find(".args([\"--no-print-directory\", \"ship-tools\", dependencies])")
        .unwrap();
    assert!(stretched_check < build);
    assert!(build < recording);
    let speech = include_str!("../../showcase/narration/mod.rs");
    assert!(
        speech.contains("const TTS_URL: &str = \"https://openrouter.ai/api/v1/audio/speech\";")
    );
    assert!(!source.contains("AOE_TTS_URL"));
    assert!(!speech.contains("AOE_TTS_URL"));
    let docs = include_str!("../../../../../../docs/showcase.md");
    assert!(!docs.contains("AOE_TTS_URL"));
}

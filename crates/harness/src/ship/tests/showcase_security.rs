use super::super::showcase::{Storyboard, create_work_dir, narrate};
use std::{
    os::unix::{fs::PermissionsExt, fs::symlink},
    sync::Mutex,
};

static ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn showcase_work_directory_is_random_private_and_ignores_legacy_pid_path_entries() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let protected = outside.path().join("protected");
    std::fs::write(&protected, b"preserve").unwrap();
    let old = root
        .path()
        .join(format!(".cache/tmp/showcase-{}", std::process::id()));
    std::fs::create_dir_all(&old).unwrap();
    symlink(&protected, old.join("plan.json")).unwrap();

    let work = create_work_dir(root.path()).unwrap();
    assert_ne!(work.path(), old);
    assert_eq!(
        work.path().parent().unwrap(),
        root.path().join(".cache/tmp")
    );
    assert!(
        work.path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("showcase-")
    );
    assert_eq!(
        std::fs::metadata(work.path()).unwrap().permissions().mode() & 0o777,
        0o700
    );
    std::fs::write(work.path().join("plan.json"), b"safe plan").unwrap();
    assert_eq!(std::fs::read(&protected).unwrap(), b"preserve");

    let linked_cache = tempfile::tempdir().unwrap();
    symlink(outside.path(), linked_cache.path().join(".cache")).unwrap();
    assert!(create_work_dir(linked_cache.path()).is_err());

    let linked_tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir(linked_tmp.path().join(".cache")).unwrap();
    symlink(outside.path(), linked_tmp.path().join(".cache/tmp")).unwrap();
    assert!(create_work_dir(linked_tmp.path()).is_err());
}

#[test]
fn narration_stub_stops_requests_when_the_five_minute_plan_is_impossible() {
    let _guard = ENV_LOCK.lock().unwrap();
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    let calls = root.path().join("calls");
    let curl = bin.join("curl");
    std::fs::write(
        &curl,
        "#!/bin/sh\nprintf x >> \"$SHOWCASE_TTS_CALLS\"\nhead -c \"$SHOWCASE_TTS_BYTES\" /dev/zero\n",
    )
    .unwrap();
    std::fs::set_permissions(&curl, std::fs::Permissions::from_mode(0o700)).unwrap();
    let prior_path = std::env::var_os("PATH");
    let prior_key = std::env::var_os("OPENROUTER_API_KEY");
    let prior_calls = std::env::var_os("SHOWCASE_TTS_CALLS");
    let prior_bytes = std::env::var_os("SHOWCASE_TTS_BYTES");
    unsafe {
        let existing_path = prior_path.as_deref().unwrap_or_default().to_string_lossy();
        std::env::set_var("PATH", format!("{}:{existing_path}", bin.display()));
        std::env::set_var("OPENROUTER_API_KEY", "stub-only");
        std::env::set_var("SHOWCASE_TTS_CALLS", &calls);
        std::env::set_var("SHOWCASE_TTS_BYTES", "7200000");
    }
    let board = Storyboard::parse(
        r#"{"title":"budget","scenes":[
          {"kind":"card","heading":"one","narration":"one"},
          {"kind":"card","heading":"two","narration":"two"},
          {"kind":"card","heading":"three","narration":"three"},
          {"kind":"card","heading":"four","narration":"four"}
        ]}"#,
    )
    .unwrap();
    let work = root.path().join("work");
    std::fs::create_dir(&work).unwrap();
    let error = narrate(&work, &board).unwrap_err().to_string();
    assert!(error.contains("showcase duration budget"), "{error}");
    assert_eq!(std::fs::read_to_string(&calls).unwrap().len(), 2);
    std::fs::write(&calls, "").unwrap();
    unsafe { std::env::set_var("SHOWCASE_TTS_BYTES", "33554432") };
    let scenes = (0..100)
        .map(|index| {
            format!(
                r#"{{"kind":"terminal","tag":"s{index}","caption":"","lines":[],"narration":"x"}}"#
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let board =
        Storyboard::parse(&format!(r#"{{"title":"aggregate","scenes":[{scenes}]}}"#)).unwrap();
    let aggregate_work = root.path().join("aggregate-work");
    std::fs::create_dir(&aggregate_work).unwrap();
    let error = narrate(&aggregate_work, &board).unwrap_err().to_string();
    assert!(error.contains("14448000-byte limit"), "{error}");
    assert_eq!(
        std::fs::metadata(aggregate_work.join("voice-0.pcm"))
            .unwrap()
            .len(),
        14_448_000
    );
    assert_eq!(std::fs::read_to_string(&calls).unwrap().len(), 1);
    unsafe {
        match prior_path {
            Some(value) => std::env::set_var("PATH", value),
            None => std::env::remove_var("PATH"),
        }
        match prior_key {
            Some(value) => std::env::set_var("OPENROUTER_API_KEY", value),
            None => std::env::remove_var("OPENROUTER_API_KEY"),
        }
        match prior_calls {
            Some(value) => std::env::set_var("SHOWCASE_TTS_CALLS", value),
            None => std::env::remove_var("SHOWCASE_TTS_CALLS"),
        }
        match prior_bytes {
            Some(value) => std::env::set_var("SHOWCASE_TTS_BYTES", value),
            None => std::env::remove_var("SHOWCASE_TTS_BYTES"),
        }
    }
}

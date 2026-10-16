use super::{Environment, mode};
use crate::ship::{showcase::make, tests::showcase_security::ENV_LOCK};

fn calls(root: &std::path::Path) -> usize {
    std::fs::read_to_string(root.join("invocations.log"))
        .unwrap()
        .lines()
        .filter(|line| line.starts_with("curl "))
        .count()
}

#[test]
fn narration_reuses_text_voice_and_model_across_scenes_and_takes() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_env, temp) = Environment::setup();
    let board = temp.path().join("storyboard.json");
    let text = std::fs::read_to_string(&board).unwrap().replace(
        "\"caption\":\"It works\"",
        "\"caption\":\"It works\",\"narration\":\"A narrated opening.\"",
    );
    std::fs::write(&board, &text).unwrap();
    make(temp.path()).unwrap();
    assert_eq!(
        calls(temp.path()),
        1,
        "identical scenes share one TTS request"
    );
    // A hit must not consult credentials or invoke curl at all.
    mode("OPENROUTER_API_KEY", "");
    mode("SHOWCASE_CURL_MODE", "fail");
    make(temp.path()).unwrap();
    assert_eq!(calls(temp.path()), 1, "another take reuses persisted PCM");
    mode("OPENROUTER_API_KEY", "stub-key");
    mode("SHOWCASE_CURL_MODE", "ok");
    std::fs::write(
        &board,
        text.replace("A narrated opening.", "Different words."),
    )
    .unwrap();
    make(temp.path()).unwrap();
    assert_eq!(calls(temp.path()), 2, "changed text misses");
    let text = text.replace(
        "\"title\":\"Pipeline\"",
        "\"title\":\"Pipeline\",\"voice\":\"Puck\"",
    );
    std::fs::write(&board, text).unwrap();
    make(temp.path()).unwrap();
    assert_eq!(calls(temp.path()), 3, "changed voice misses");
}

#[test]
fn failed_or_invalid_speech_never_poison_the_next_take() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for failure in ["fail", "empty", "odd"] {
        let (_env, temp) = Environment::setup();
        mode("SHOWCASE_CURL_MODE", failure);
        assert!(make(temp.path()).is_err(), "{failure}");
        mode("SHOWCASE_CURL_MODE", "ok");
        make(temp.path()).unwrap();
        assert_eq!(
            calls(temp.path()),
            2,
            "{failure}: the failure was not cached"
        );
        make(temp.path()).unwrap();
        assert_eq!(calls(temp.path()), 2, "the successful retry is cached");
    }
}

#[test]
fn cached_audio_cannot_follow_a_symlink_or_escape_the_cache() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_env, temp) = Environment::setup();
    make(temp.path()).unwrap();
    let entry = std::fs::read_dir(temp.path().join(".cache/showcase-tts"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let protected = temp.path().join("protected.pcm");
    std::fs::rename(&entry, &protected).unwrap();
    std::os::unix::fs::symlink(&protected, &entry).unwrap();
    assert!(make(temp.path()).is_err());
    assert_eq!(std::fs::read(&protected).unwrap(), [1, 2, 3, 4]);
    assert_eq!(calls(temp.path()), 1);
}

#[test]
fn a_cache_hit_still_obeys_the_selected_duration_budget() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_env, temp) = Environment::setup();
    mode("SHOWCASE_CURL_MODE", "long");
    let error = make(temp.path()).unwrap_err().to_string();
    assert!(error.contains("duration budget"), "{error}");
    assert_eq!(calls(temp.path()), 1);
    mode("SHOWCASE_CURL_MODE", "fail");
    let error = make(temp.path()).unwrap_err().to_string();
    assert!(error.contains("duration budget"), "cached audio: {error}");
    assert_eq!(
        calls(temp.path()),
        1,
        "cache hits enforce the cap without speech calls"
    );
}

fn installed_dependencies(root: &std::path::Path) {
    let browser = root.join("browser");
    let modules = root.join("modules");
    std::fs::create_dir_all(&browser).unwrap();
    std::fs::write(
        browser.join("package.json"),
        r#"{"devDependencies":{"playwright":"1.63.0"}}"#,
    )
    .unwrap();
    let package = serde_json::json!({"version":"1.63.0", "dev":true});
    let lock = serde_json::json!({"lockfileVersion":3,"packages":{
        "":{"devDependencies":{"playwright":"1.63.0"}},
        "node_modules/playwright":package,
    }});
    let hidden =
        serde_json::json!({"lockfileVersion":3,"packages":{"node_modules/playwright":package}});
    std::fs::write(browser.join("package-lock.json"), lock.to_string()).unwrap();
    std::fs::write(modules.join(".package-lock.json"), hidden.to_string()).unwrap();
    std::fs::write(
        modules.join("playwright/package.json"),
        r#"{"version":"1.63.0"}"#,
    )
    .unwrap();
}

#[test]
fn unchanged_installed_dependencies_skip_npm_but_drift_requires_it() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for drift in ["none", "lock", "manifest", "missing", "version", "metadata"] {
        let (_env, temp) = Environment::setup();
        installed_dependencies(temp.path());
        match drift {
            "lock" => {
                let path = temp.path().join("browser/package-lock.json");
                let mut lock: serde_json::Value =
                    serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
                lock["packages"]["node_modules/playwright"]["integrity"] =
                    "changed content, same version".into();
                std::fs::write(path, lock.to_string()).unwrap();
            }
            "manifest" => std::fs::write(
                temp.path().join("browser/package.json"),
                r#"{"devDependencies":{"playwright":"1.64.0"}}"#,
            )
            .unwrap(),
            "missing" => {
                std::fs::remove_file(temp.path().join("modules/playwright/package.json")).unwrap()
            }
            "version" => std::fs::write(
                temp.path().join("modules/playwright/package.json"),
                r#"{"version":"1.64.0"}"#,
            )
            .unwrap(),
            "metadata" => std::fs::write(
                temp.path().join("modules/.package-lock.json"),
                "broken JSON",
            )
            .unwrap(),
            _ => (),
        }
        make(temp.path()).unwrap();
        let log = std::fs::read_to_string(temp.path().join("invocations.log")).unwrap();
        if drift == "none" {
            assert!(
                log.contains("make --no-print-directory ship-tools browser-tools"),
                "{log}"
            );
            assert!(!log.contains("browser-deps"), "installed lock matches");
        } else {
            assert!(
                log.contains("make --no-print-directory ship-tools browser-deps"),
                "{drift}: {log}"
            );
        }
    }
}

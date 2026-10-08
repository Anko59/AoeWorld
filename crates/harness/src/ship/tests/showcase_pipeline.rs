use super::{
    super::showcase::{check, make},
    showcase_security::ENV_LOCK,
};
use std::{os::unix::fs::PermissionsExt, path::Path};

const BOARD: &str = r#"{"title":"Pipeline","scenes":[
  {"kind":"card","heading":"Opening","narration":"A narrated opening."},
  {"kind":"terminal","tag":"check","caption":"It works","lines":[{"style":"good","text":"passed"}]}
]}"#;

const ENV: &[&str] = &[
    "PATH",
    "MAKE",
    "SHOWCASE_STORYBOARD",
    "SHOWCASE_OUT",
    "SHOWCASE_LEVEL",
    "SHIP_LEVEL",
    "AOE_BROWSER_IMAGE",
    "AOE_SHIP_TOOLS_IMAGE",
    "AOE_SHOWCASE_NODE_MODULES",
    "OPENROUTER_API_KEY",
    "SHOWCASE_LOG",
    "SHOWCASE_CURL_MODE",
    "SHOWCASE_DOCKER_MODE",
    "SHOWCASE_MAKE_MODE",
];

struct Environment(Vec<(&'static str, Option<std::ffi::OsString>)>);

impl Environment {
    fn setup() -> (Self, tempfile::TempDir) {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.cache/tmp");
        std::fs::create_dir_all(&root).unwrap();
        let temp = tempfile::Builder::new()
            .prefix("showcase-test-")
            .tempdir_in(root)
            .unwrap();
        let saved = ENV
            .iter()
            .map(|name| (*name, std::env::var_os(name)))
            .collect();
        let env = Self(saved);
        let bin = temp.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        executable(
            &bin.join("curl"),
            r#"#!/bin/sh
printf 'curl %s\n' "$*" >> "$SHOWCASE_LOG"
cat >> "$SHOWCASE_LOG"
printf '\n--response--\n' >> "$SHOWCASE_LOG"
case "$SHOWCASE_CURL_MODE" in
  fail) printf 'provider refused request'; exit 22 ;;
  large) head -c 33554433 /dev/zero ;;
  empty) : ;;
  *) printf '\001\002\003\004' ;;
esac
"#,
        );
        executable(
            &bin.join("docker"),
            r#"#!/bin/sh
printf 'docker %s\n' "$*" >> "$SHOWCASE_LOG"
case "$SHOWCASE_DOCKER_MODE" in fail) echo 'stub docker failed' >&2; exit 9;; esac
is_recorder=0
is_ffmpeg=0
for arg do
  [ "$arg" = 'record.mjs' ] && is_recorder=1
  [ "$arg" = 'ffmpeg' ] && is_ffmpeg=1
done
if [ "$is_recorder" = 1 ]; then
    work=''
    previous=''
    for arg in "$@"; do
      if [ "$previous" = '-w' ]; then work=$arg; fi
      previous=$arg
    done
    case "$SHOWCASE_DOCKER_MODE" in no-video) exit 0;; esac
    printf '{"lead_in_ms":10,"scenes_ms":[20,30]}' > "$work/timings.json"
    printf 'silent-video' > "$work/silent.webm"
elif [ "$is_ffmpeg" = 1 ]; then
    case "$SHOWCASE_DOCKER_MODE" in mix-fail) echo 'stub ffmpeg failed' >&2; exit 8;; esac
    for output do :; done
    printf 'encoded:%s' "$output" > "$output"
fi
"#,
        );
        executable(
            &bin.join("make"),
            r#"#!/bin/sh
printf 'make %s\n' "$*" >> "$SHOWCASE_LOG"
case "$SHOWCASE_MAKE_MODE" in fail) exit 3;; esac
"#,
        );
        let modules = temp.path().join("modules");
        std::fs::create_dir_all(modules.join("playwright")).unwrap();
        let board = temp.path().join("storyboard.json");
        std::fs::write(&board, BOARD).unwrap();
        unsafe {
            std::env::set_var(
                "PATH",
                format!(
                    "{}:{}",
                    bin.display(),
                    std::env::var("PATH").unwrap_or_default()
                ),
            );
            std::env::set_var("MAKE", "make");
            std::env::set_var("SHOWCASE_STORYBOARD", &board);
            std::env::set_var("SHOWCASE_OUT", ".cache/showcase/final.webm");
            std::env::set_var("SHOWCASE_LEVEL", "high");
            std::env::remove_var("SHIP_LEVEL");
            std::env::set_var("AOE_BROWSER_IMAGE", "browser:test");
            std::env::set_var("AOE_SHIP_TOOLS_IMAGE", "tools:test");
            std::env::set_var("AOE_SHOWCASE_NODE_MODULES", modules);
            std::env::set_var("OPENROUTER_API_KEY", "stub-key");
            std::env::set_var("SHOWCASE_LOG", temp.path().join("invocations.log"));
            std::env::remove_var("SHOWCASE_CURL_MODE");
            std::env::remove_var("SHOWCASE_DOCKER_MODE");
            std::env::remove_var("SHOWCASE_MAKE_MODE");
        }
        (env, temp)
    }
}

impl Drop for Environment {
    fn drop(&mut self) {
        unsafe {
            for (name, value) in self.0.drain(..) {
                match value {
                    Some(value) => std::env::set_var(name, value),
                    None => std::env::remove_var(name),
                }
            }
        }
    }
}

fn executable(path: &Path, script: &str) {
    std::fs::write(path, script).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
}

fn mode(name: &str, value: &str) {
    unsafe { std::env::set_var(name, value) }
}

#[test]
fn check_and_make_drive_narration_recording_mix_and_atomic_publish() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_env, temp) = Environment::setup();
    check(temp.path()).unwrap();
    let output = make(temp.path()).unwrap();
    assert_eq!(
        output,
        temp.path()
            .canonicalize()
            .unwrap()
            .join(".cache/showcase/final.webm")
    );
    assert!(std::fs::read(&output).unwrap().starts_with(b"encoded:"));
    let log = std::fs::read_to_string(temp.path().join("invocations.log")).unwrap();
    assert!(log.contains("Authorization: Bearer stub-key"));
    assert!(log.contains("make --no-print-directory ship-tools browser-deps"));
    assert!(log.contains("browser:test node record.mjs"));
    assert_eq!(log.matches("docker run").count(), 4);
    assert!(log.contains("-f s16le -ar 24000 -ac 1"));
    assert!(log.contains("-f concat -safe 0"));
    let parent = output.parent().unwrap();
    assert_eq!(std::fs::read_dir(parent).unwrap().count(), 1);
}

#[test]
fn an_unvoiced_showcase_publishes_the_recording_without_a_mix() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_env, temp) = Environment::setup();
    unsafe { std::env::set_var("SHOWCASE_LEVEL", "medium") };
    std::fs::write(
        temp.path().join("storyboard.json"),
        r#"{"title":"Silent","scenes":[
          {"kind":"card","heading":"Opening"},
          {"kind":"terminal","tag":"check","caption":"It works","lines":[]}
        ]}"#,
    )
    .unwrap();
    let output = make(temp.path()).unwrap();
    assert_eq!(std::fs::read(output).unwrap(), b"silent-video");
    let log = std::fs::read_to_string(temp.path().join("invocations.log")).unwrap();
    assert_eq!(log.matches("docker run").count(), 1);
    assert!(!log.contains("curl "));
}

#[test]
fn showcase_reports_process_failures_and_missing_recorder_outputs() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for (variable, value, expected) in [
        (
            "SHOWCASE_MAKE_MODE",
            "fail",
            "building showcase images failed",
        ),
        ("SHOWCASE_DOCKER_MODE", "fail", "the recorder failed"),
        (
            "SHOWCASE_DOCKER_MODE",
            "no-video",
            "No such file or directory",
        ),
        ("SHOWCASE_DOCKER_MODE", "mix-fail", "stub ffmpeg failed"),
    ] {
        let (_env, temp) = Environment::setup();
        mode(variable, value);
        let error = make(temp.path()).unwrap_err().to_string();
        assert!(error.contains(expected), "{variable}={value}: {error}");
    }
}

#[test]
fn narration_rejects_curl_errors_empty_and_oversized_pcm_responses() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for (value, expected) in [
        ("fail", "provider refused request"),
        ("empty", "no audio"),
        ("large", "speech response exceeds the 14448000-byte limit"),
    ] {
        let (_env, temp) = Environment::setup();
        mode("SHOWCASE_CURL_MODE", value);
        let error = make(temp.path()).unwrap_err().to_string();
        assert!(error.contains(expected), "mode={value}: {error}");
    }
}

#[test]
fn recording_refuses_to_start_when_the_playwright_mount_is_missing() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_env, temp) = Environment::setup();
    unsafe {
        std::env::set_var(
            "AOE_SHOWCASE_NODE_MODULES",
            temp.path().join("missing-modules"),
        );
    }
    let error = make(temp.path()).unwrap_err().to_string();
    assert!(error.contains("has no Playwright"), "{error}");
    let log = std::fs::read_to_string(temp.path().join("invocations.log")).unwrap();
    assert!(log.contains("make --no-print-directory ship-tools browser-deps"));
    assert!(!log.contains("docker "));
}

#[test]
fn check_refuses_a_voiced_level_without_narration_before_side_effects() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_env, temp) = Environment::setup();
    std::fs::write(
        temp.path().join("storyboard.json"),
        r#"{"title":"silent","scenes":[{"kind":"card","heading":"Hello"}]}"#,
    )
    .unwrap();
    let error = check(temp.path()).unwrap_err().to_string();
    assert!(error.contains("high showcases need at least one narrated scene"));
    assert!(!temp.path().join("invocations.log").exists());
}

#[test]
fn check_reads_ship_level_and_pull_request_template_fallbacks() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_env, temp) = Environment::setup();
    unsafe {
        std::env::remove_var("SHOWCASE_LEVEL");
        std::env::remove_var("SHIP_LEVEL");
    }
    let missing = check(temp.path()).unwrap_err().to_string();
    assert!(missing.contains("SHOWCASE_LEVEL is required"));

    unsafe { std::env::set_var("SHIP_LEVEL", "medium") };
    check(temp.path()).unwrap();
    unsafe { std::env::remove_var("SHIP_LEVEL") };

    let github = temp.path().join(".github");
    std::fs::create_dir(&github).unwrap();
    std::fs::write(
        github.join("pull_request_template.md"),
        "Describe the change.\n<!-- level: medium -->\n",
    )
    .unwrap();
    check(temp.path()).unwrap();
}

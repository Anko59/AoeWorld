//! A stubbed `gh` first on PATH and a checkout whose origin is
//! github.com/project/checkout, under the issue tests' environment lock.
use super::tests::ENVIRONMENT;
use std::{
    ffi::OsString, fs, os::unix::fs::PermissionsExt, path::PathBuf, process::Command,
    sync::MutexGuard,
};

const GH: &str = r#"#!/bin/sh
printf -- '---\n' >> "$GH_LOG"
printf '<%s>\n' "$@" >> "$GH_LOG"
for arg in "$@"; do
  if [ -n "${GH_FAIL-}" ] && [ "$arg" = "$GH_FAIL" ]; then
    # A failing gh has still read its piped body: drain it before exiting.
    case " $* " in *" --body-file - "*) cat > /dev/null ;; esac
    echo "stub failure" >&2; exit 1
  fi
done
case "$1" in
  --version) printf 'gh version 2.100.0\n' ;;
  api) printf '%s' "$GH_LIST_JSON" ;;
  issue) case "$2" in
    create|edit)
      cat >> "$GH_BODY_LOG"; printf -- '\n===\n' >> "$GH_BODY_LOG"
      if [ "$2" = create ]; then printf 'https://github.com/project/checkout/issues/99\n'; fi ;;
    view)
      if [ -n "${GH_VIEW_RESPONSES-}" ]; then
        count=$(cat "$GH_VIEW_RESPONSES/count")
        count=$((count + 1))
        printf '%s' "$count" > "$GH_VIEW_RESPONSES/count"
        cat "$GH_VIEW_RESPONSES/$count"
      else
        case " $* " in
          *" body,updatedAt "*) printf '%s' "$GH_VIEW_JSON" ;;
          *" updatedAt "*) printf '2026-10-14T00:00:00Z' ;;
          *) printf '%s' "${GH_VIEW_BODY-}" ;;
        esac
      fi ;;
    # Like the real gh, read a body piped on stdin (`--body-file -`) to its
    # end: exiting first closes the pipe under the writer (EPIPE).
    *) case " $* " in *" --body-file - "*) cat > /dev/null ;; esac ;;
  esac ;;
  pr) if [ "$2" = list ]; then printf '%s' "${GH_OPEN_PR-}"; fi
    case " $* " in *" --body-file - "*) cat >> "$GH_BODY_LOG"; printf -- '\n===\n' >> "$GH_BODY_LOG" ;; esac ;;
esac
"#;

pub(super) struct Stub {
    _guard: MutexGuard<'static, ()>,
    _temp: tempfile::TempDir,
    pub(super) root: PathBuf,
    log: PathBuf,
    bodies: PathBuf,
    saved: Vec<(&'static str, Option<OsString>)>,
}

impl Stub {
    /// `listing` is what `gh api` prints for the open issues.
    pub(super) fn new(listing: &serde_json::Value) -> Self {
        let guard = ENVIRONMENT.lock().unwrap_or_else(|e| e.into_inner());
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("checkout");
        fs::create_dir_all(&root).unwrap();
        for args in [
            &["init", "-q"][..],
            &[
                "remote",
                "add",
                "origin",
                "https://github.com/project/checkout.git",
            ],
        ] {
            assert!(
                Command::new("git")
                    .args(args)
                    .current_dir(&root)
                    .status()
                    .unwrap()
                    .success()
            );
        }
        let bin = temp.path().join("bin");
        fs::create_dir(&bin).unwrap();
        fs::write(bin.join("gh"), GH).unwrap();
        fs::set_permissions(bin.join("gh"), fs::Permissions::from_mode(0o755)).unwrap();
        let log = temp.path().join("gh.log");
        let bodies = temp.path().join("bodies.log");
        fs::write(&log, "").unwrap();
        fs::write(&bodies, "").unwrap();
        let path = std::env::var_os("PATH").unwrap_or_default();
        let mut stub = Self {
            _guard: guard,
            _temp: temp,
            root,
            log,
            bodies,
            saved: Vec::new(),
        };
        stub.set(
            "PATH",
            &format!("{}:{}", bin.display(), path.to_string_lossy()),
        );
        stub.set("GH_LOG", &stub.log.display().to_string());
        stub.set("GH_BODY_LOG", &stub.bodies.display().to_string());
        stub.set("GH_LIST_JSON", &listing.to_string());
        stub.set("GITHUB_REPOSITORY", "wrong/override");
        stub.set("GH_REPO", "wrong/override");
        stub.unset("GH_FAIL");
        stub.unset("GH_VIEW_BODY");
        stub.unset("GH_VIEW_RESPONSES");
        stub.unset("GH_OPEN_PR");
        stub.set(
            "GH_VIEW_JSON",
            r#"{"body":"","updatedAt":"2026-10-14T00:00:00Z"}"#,
        );
        stub.unset("AOE_AGENT_ROLE");
        stub
    }

    pub(super) fn set(&mut self, name: &'static str, value: &str) {
        if name == "GH_VIEW_BODY" {
            self.set(
                "GH_VIEW_JSON",
                &serde_json::json!({
                    "body": value, "updatedAt": "2026-10-14T00:00:00Z"
                })
                .to_string(),
            );
        }
        self.saved.push((name, std::env::var_os(name)));
        unsafe { std::env::set_var(name, value) };
    }

    pub(super) fn view_responses(&mut self, responses: &[String]) {
        let directory = self._temp.path().join("views");
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("count"), "0").unwrap();
        for (index, response) in responses.iter().enumerate() {
            fs::write(directory.join((index + 1).to_string()), response).unwrap();
        }
        self.set("GH_VIEW_RESPONSES", directory.to_str().unwrap());
    }

    pub(super) fn unset(&mut self, name: &'static str) {
        self.saved.push((name, std::env::var_os(name)));
        unsafe { std::env::remove_var(name) };
    }

    /// Every `gh` call so far, one `<arg>` per line, calls split by `---`.
    pub(super) fn calls(&self) -> String {
        fs::read_to_string(&self.log).unwrap()
    }

    /// The bodies `gh issue create` read on standard input, split by `===`.
    pub(super) fn bodies(&self) -> String {
        fs::read_to_string(&self.bodies).unwrap()
    }

    pub(super) fn clear(&self) {
        fs::write(&self.log, "").unwrap();
        fs::write(&self.bodies, "").unwrap();
    }
}

impl Drop for Stub {
    fn drop(&mut self) {
        for (name, value) in self.saved.drain(..).rev() {
            match value {
                Some(value) => unsafe { std::env::set_var(name, value) },
                None => unsafe { std::env::remove_var(name) },
            }
        }
    }
}

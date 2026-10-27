use super::*;

const PASS: &str = r#"{"revision":"abc","pass":true,"failures":[],"ticks":{"p50_ms":1.2,"p95_ms":4.5,"max_ms":12.0},"session_drops":0,"scenarios":[{"name":"gather","pass":true,"detail":"done"}],"recording":"match.webm"}"#;
const FAIL: &str = r#"{"revision":"abc","pass":false,"failures":["lost"],"ticks":{"p50_ms":1.0,"p95_ms":2.0,"max_ms":3.0},"session_drops":1,"scenarios":[{"name":"attack","pass":false,"detail":"failed"}]}"#;

#[test]
fn contract_and_rendering() {
    let pass = Report::parse(PASS, "abc").unwrap();
    assert!(pass.pass);
    assert!(Report::parse(PASS, "different").is_none());
    let fail = Report::parse(FAIL, "abc").unwrap();
    assert!(!fail.pass);
    let section = fail.section();
    for text in [
        "Playtest (advisory)",
        "FAIL",
        "attack",
        "failed",
        "2 ms",
        "1",
        "lost",
    ] {
        assert!(section.contains(text), "{section}: missing {text}");
    }
    assert!(pass.section().contains("match.webm"));
    assert!(pass.summary().contains("1/1 scenarios pass"));
}

#[test]
fn gameplay_paths_have_directory_boundaries() {
    for path in [
        "crates/simulation/a",
        "crates/server/a",
        "crates/client/a",
        "crates/sdk/a",
        "sdk/a",
        "ai/a",
        "crates/protocol/a",
    ] {
        assert!(gameplay(path), "{path}");
    }
    for path in ["docs/a", "crates/serverless/a", "a/ai/a"] {
        assert!(!gameplay(path), "{path}");
    }
}

#[cfg(unix)]
#[test]
fn scripted_make_without_target_is_unavailable() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let make = dir.path().join("make");
    std::fs::write(&make, "#!/bin/sh\nprintf 'other:\n'\n").unwrap();
    std::fs::set_permissions(&make, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(!target_available(dir.path(), make.to_str().unwrap()));
    std::fs::write(&make, "#!/bin/sh\nprintf 'playtest:\n'\n").unwrap();
    assert!(target_available(dir.path(), make.to_str().unwrap()));
}

#[test]
fn ship_continues_when_gameplay_has_no_playtest_target() {
    use crate::ship::{
        git, ship,
        tests::{fixture, offline, run},
    };
    let (_temp, root) = fixture("true");
    std::fs::create_dir_all(root.join("ai")).unwrap();
    std::fs::write(root.join("ai/change.py"), "change\n").unwrap();
    run(&root, &["add", "ai/change.py"]);
    run(&root, &["commit", "-q", "-m", "gameplay"]);
    let head = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    let result = ship(&root, &offline()).expect("advisory playtest must not block ship");
    assert_eq!(result.head, head);
}

/// A gameplay commit in a ship fixture, a scripted `make` with a `playtest` target running
/// `body`, and the passing-review options. `body` may use `$HEAD` and writes in the fixture root.
#[cfg(unix)]
fn scripted(
    body: &str,
) -> (
    tempfile::TempDir,
    std::path::PathBuf,
    String,
    crate::ship::Options,
) {
    use crate::ship::{
        git, lock,
        tests::{fixture, offline, run},
    };
    use std::os::unix::fs::PermissionsExt;
    let (temp, root) = fixture("true");
    std::fs::create_dir_all(root.join("ai")).unwrap();
    std::fs::write(root.join("ai/change.py"), "change\n").unwrap();
    run(&root, &["add", "ai/change.py"]);
    run(&root, &["commit", "-q", "-m", "gameplay"]);
    let head = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    let make = temp.path().join("make");
    std::fs::write(
        &make,
        format!(
            "#!/bin/sh\nif [ \"$1\" = -rRqp ]; then echo 'playtest:'; exit 0; fi\n\
             HEAD={head}\nMARK={mark}\necho \"$@\" > \"$MARK/invoked\"\n{body}\n",
            mark = temp.path().display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&make, std::fs::Permissions::from_mode(0o755)).unwrap();
    // Reports and logs stay out of the status the ship re-check reads, as in the real repository.
    std::fs::write(root.join(".git/info/exclude"), "/reports/\n").unwrap();
    MAKE.with(|slot| *slot.borrow_mut() = Some(make.to_string_lossy().into_owned()));
    let options = crate::ship::Options {
        no_review: false,
        lock: lock::Handoff::holding(lock::acquire(&root, "make ship").unwrap()),
        ..offline()
    };
    (temp, root, head, options)
}

#[cfg(unix)]
fn appears(path: &std::path::Path) -> bool {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while !path.exists() && std::time::Instant::now() < deadline {
        thread::sleep(Duration::from_millis(50));
    }
    path.exists()
}

#[cfg(unix)]
#[test]
fn playtest_is_a_child_make_running_alongside_the_review() {
    let (temp, root, head, options) = scripted(
        "sleep 1\nmkdir -p reports/playtest\n\
         printf '{\"revision\":\"%s\",\"pass\":true,\"failures\":[],\"ticks\":{\"p50_ms\":1,\"p95_ms\":2,\"max_ms\":3},\"session_drops\":0,\"scenarios\":[]}' \"$HEAD\" > \"reports/playtest/$HEAD.json\"",
    );
    let result = crate::ship::ship_with(&root, &options, &|root, tier, _, _, _| {
        // The review runs while the playtest child does: no in-process slot wait comes first.
        assert!(
            appears(&temp.path().join("invoked")),
            "make playtest was not started"
        );
        assert!(!root.join(format!("reports/playtest/{head}.json")).exists());
        Ok(crate::ship::review_gate::graded(root, tier, 9))
    });
    assert_eq!(result.expect("ship passes").head, head);
    let invoked = std::fs::read_to_string(temp.path().join("invoked")).unwrap();
    assert_eq!(invoked.trim(), "playtest");
    assert!(root.join(format!("reports/playtest/{head}.json")).exists());
}

#[cfg(unix)]
#[test]
fn failed_review_returns_at_once_and_kills_the_playtest() {
    let (temp, root, _head, options) =
        scripted("sleep 60 &\necho $! > \"$MARK/sleeper\"\necho $$ > \"$MARK/pid\"\nwait");
    let mark = temp.path();
    let started = std::time::Instant::now();
    let error = crate::ship::ship_with(&root, &options, &|_, _, _, _, _| {
        assert!(appears(&mark.join("sleeper")) && appears(&mark.join("pid")));
        Err("review exploded".into())
    })
    .unwrap_err();
    assert!(error.to_string().contains("review exploded"), "{error}");
    assert!(
        started.elapsed() < Duration::from_secs(30),
        "{:?}",
        started.elapsed()
    );
    for file in ["pid", "sleeper"] {
        let pid = std::fs::read_to_string(mark.join(file)).unwrap();
        let stat = std::fs::read_to_string(format!("/proc/{}/stat", pid.trim()));
        // Gone, or a zombie awaiting its reaper: either way no longer running.
        let running = stat.is_ok_and(|stat| {
            !stat
                .rsplit_once(')')
                .is_some_and(|(_, rest)| rest.trim_start().starts_with('Z'))
        });
        assert!(!running, "{file} {pid} still runs");
    }
}

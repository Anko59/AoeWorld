//! Rehearse the branch binary's ship command against a local bare remote
//! and a scripted GitHub, without publishing anything to GitHub.
use super::*;
use crate::ship::{git, tests::fixture};
use std::{fs, io::Write, os::unix::fs::PermissionsExt, process::Command};

#[test]
fn the_ship_command_preserves_concurrent_ticks_and_comments_when_busy() {
    let section = plan(42, PR, &minor("other.rs", 1)).section.unwrap();
    let (listing, stale) = listed_with_standing(&[], &section);
    let fresh = stale.replace("- [ ]", "- [x]");
    let mut stub = Stub::new(&listing);
    let (temp, root) = fixture("true");
    // Script only the GitHub URL lookup; all Git transport uses the real
    // fixture's local bare remote. This PATH is confined to the CLI child.
    let real_git = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|path| path.join("git"))
        .find(|path| path.is_file())
        .unwrap();
    let bin = temp.path().join("cli-bin");
    fs::create_dir(&bin).unwrap();
    let wrapper = bin.join("git");
    fs::write(&wrapper, format!(
        "#!/bin/sh\nif [ \"$3 $4 $5\" = 'remote get-url origin' ]; then\n  printf 'https://github.com/project/checkout.git\\n'\nelse\n  exec '{}' \"$@\"\nfi\n", real_git.display()
    )).unwrap();
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o755)).unwrap();
    let cli_path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
    stub.set(
        "GH_OPEN_PR",
        &serde_json::json!({"url": OTHER_PR, "baseRefName": "dev"}).to_string(),
    );
    let task = "<!-- level: low -->\n## Why\n> \"preserve concurrent ticks\" — the user\n## What\nExercise the follow-up update.\n";
    let body = temp.path().join("body.md");
    fs::write(&body, task).unwrap();
    let mut reviewed = minor("new.rs", 8);
    reviewed.head = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    reviewed.base = git::git(&root, &["rev-parse", "refs/remotes/origin/dev"]).unwrap();
    reviewed.merge_base = reviewed.base.clone();
    reviewed.task_fingerprint = Some(Report::task_fingerprint(task));
    reviewed.policy_fingerprint = Some(Report::policy_fingerprint(&root, "low").unwrap());
    reviewed.store(&root).unwrap();
    let binary = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("aoe-harness");
    for busy in [false, true] {
        stub.clear();
        let responses: Vec<String> = if busy {
            (0..3)
                .flat_map(|n| [snapshot(&fresh, &n.to_string()), (n + 1).to_string()])
                .collect()
        } else {
            vec![
                snapshot(&stale, "first"),
                "second".into(),
                snapshot(&fresh, "second"),
                "second".into(),
            ]
        };
        stub.view_responses(&responses);
        let output = Command::new(&binary)
            .current_dir(&root)
            .args(["ship", "--body-file"])
            .arg(&body)
            .env("PATH", &cli_path)
            .env_remove("AOE_JUDGE_REV")
            .env_remove("SHIP_BASE")
            .env_remove("SHIP_VIDEO")
            .env_remove("SHIP_TIER")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{stdout}\n{stderr}");
        let calls = stub.calls();
        assert!(calls.contains("<pr>\n<comment>\n<43>"), "{calls}");
        assert_eq!(
            calls.matches("<issue>\n<view>").count(),
            if busy { 6 } else { 4 }
        );
        assert_eq!(calls.matches("<issue>\n<edit>").count(), usize::from(!busy));
        if busy {
            assert!(stderr.contains("standing issue was busy"), "{stderr}");
            assert!(stub.bodies().contains("- [ ] new.rs:8"));
        } else {
            assert!(stub.bodies().contains("- [x] other.rs:1"));
            assert!(!stub.bodies().contains("- [ ] other.rs:1"));
        }
        writeln!(std::io::stdout(), "CLI rehearsal (local remote, scripted GitHub; busy={busy}):\n{stdout}{stderr}ASSERT: {} views, {} edits; PR #43 comment posted; {}", if busy { 6 } else { 4 }, usize::from(!busy), if busy { "standing issue left untouched" } else { "PR #42 checked finding preserved" }).unwrap();
    }
}

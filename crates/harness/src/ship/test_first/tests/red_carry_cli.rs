//! Exercise the checkout binary's shipping decision in a disposable repository.
use super::commit;
use crate::ship::{
    duel::focus::CheckReport,
    git,
    test_first::red,
    tests::{fixture, run},
};
use std::{fs, io::Write, os::unix::fs::PermissionsExt, process::Command};

#[test]
fn red_carry_cli_accepts_rebased_proof_before_running_gates() {
    let _guard = crate::ship::issues::tests::ENVIRONMENT.lock().unwrap();
    // A deliberately failing fixture gate stops this rehearsal before review or publication.
    let (temp, root) = fixture("false");
    let spec = "browser/tests/carry.spec.ts";
    let name = "carry.spec.ts::new behavior [webgpu]";
    commit(
        &root,
        &[
            (spec, "test('new behavior', () => {});\n"),
            ("web/index.html", "after\n"),
        ],
        "feature",
    );
    let base = git::git(&root, &["rev-parse", "origin/dev"]).unwrap();
    let old = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    red::run_with(&root, &base, &old, &mut |snapshot, _| {
        let green = snapshot.join("web/index.html").exists();
        Ok(CheckReport {
            passed: green,
            tests: vec![name.into()],
            failed: if green { vec![] } else { vec![name.into()] },
            locations: [(name.into(), (spec.into(), 1))].into(),
            ..Default::default()
        })
    })
    .unwrap();
    run(&root, &["checkout", "-q", "dev"]);
    commit(&root, &[("upstream.md", "dev moved\n")], "dev advances");
    run(&root, &["push", "-q", "origin", "dev"]);
    run(&root, &["checkout", "-q", "feature"]);
    run(&root, &["rebase", "dev"]);
    let head = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    assert_ne!(old, head);
    let body = temp.path().join("body.md");
    fs::write(&body, "<!-- level: low -->\n## Why\n> \"reuse red after rebase\" — the user\n## What\nAccept identical patches.\n").unwrap();
    let bin = temp.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let real_git = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|dir| dir.join("git"))
        .find(|file| file.is_file())
        .unwrap();
    fs::write(bin.join("git"), format!("#!/bin/sh\nif [ \"$3 $4 $5\" = 'remote get-url origin' ]; then\n printf 'https://github.com/project/checkout.git\\n'\nelse\n exec '{}' \"$@\"\nfi\n", real_git.display())).unwrap();
    fs::write(bin.join("gh"), "#!/bin/sh\ncase \"$1 $2\" in\n '--version ') echo 'gh version 2.100.0' ;;\n 'pr list') printf '{\"url\":\"https://github.com/project/checkout/pull/43\",\"baseRefName\":\"dev\"}' ;;\n 'api '*) printf '[]' ;;\nesac\n").unwrap();
    for tool in ["git", "gh"] {
        fs::set_permissions(bin.join(tool), fs::Permissions::from_mode(0o755)).unwrap();
    }
    let binary = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("aoe-harness");
    let output = Command::new(binary)
        .current_dir(&root)
        .args(["ship", "--body-file"])
        .arg(body)
        .env(
            "PATH",
            format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
        )
        .env_remove("AOE_JUDGE_REV")
        .env_remove("SHIP_BASE")
        .env_remove("SHIP_VIDEO")
        .env_remove("SHIP_TIER")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "the fixture gate must stop publication"
    );
    assert!(
        stderr.contains(&format!(
            "test-first red: PASS (carried from {old}; patch unchanged after rebase)"
        )),
        "{stdout}\n{stderr}"
    );
    assert!(stderr.contains("ship: FAIL; fix the gates"), "{stderr}");
    assert!(
        git::git(
            &root,
            &["rev-parse", "--verify", "refs/remotes/origin/feature"]
        )
        .is_err()
    );
    // Keep the real command's output visible in the Make proof log on successful tests too.
    writeln!(
        std::io::stdout(),
        "checkout CLI rehearsal (fixture gate intentionally fails):\n{stdout}\n{stderr}"
    )
    .unwrap();
}

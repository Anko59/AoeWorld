//! Real ship fetch/re-check/push calls, with a local SSH stub and no network.
use crate::ship::{
    git, judge, recheck,
    tests::{fixture, offline, run},
};
use std::{fs, os::unix::fs::PermissionsExt, path::Path, time::Duration};

fn ssh_stub(root: &Path, scratch: &Path, error: &str, recover: bool) -> std::path::PathBuf {
    let counter = scratch.join("ssh-calls");
    let script = scratch.join("ssh-stub");
    fs::write(&script, format!(
        "#!/bin/sh\necho call >> '{}'\nn=$(wc -l < '{}')\nif [ \"$n\" = 1 ] || [ '{}' = false ]; then\n  echo '{}' >&2\n  exit 255\nfi\nfor last; do :; done\nexec sh -c \"$last\"\n",
        counter.display(), counter.display(), recover, error
    )).unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    run(
        root,
        &["config", "core.sshCommand", script.to_str().unwrap()],
    );
    run(root, &["config", "ssh.variant", "ssh"]);
    let origin = scratch.join("origin.git");
    run(
        root,
        &[
            "remote",
            "set-url",
            "origin",
            &format!("ssh://stub{}", origin.display()),
        ],
    );
    counter
}

#[test]
fn all_three_ship_network_paths_recover_from_an_agent_hiccup() {
    let _guard = crate::ship::issues::tests::ENVIRONMENT.lock().unwrap();
    for path in ["initial-fetch", "recheck-fetch", "push"] {
        let (temp, root) = fixture("true");
        let evidence = judge(&root, &offline()).unwrap();
        let counter = ssh_stub(&root, temp.path(), "Permission denied (publickey)", true);
        let (_, waits) = git::remote::without_sleep(|| match path {
            "initial-fetch" => {
                git::base(&root, "dev", true).unwrap();
            }
            "recheck-fetch" => {
                assert!(
                    recheck::before_push(&root, &evidence, None, &offline())
                        .unwrap()
                        .is_none()
                );
            }
            "push" => {
                crate::ship::push(&root, &evidence, None).unwrap();
            }
            _ => unreachable!(),
        });
        assert_eq!(waits, [Duration::from_secs(20)], "{path}");
        assert_eq!(
            fs::read_to_string(counter).unwrap().lines().count(),
            2,
            "{path}"
        );
        if path == "push" {
            let remote = temp.path().join("origin.git");
            assert_eq!(
                git::git(&remote, &["rev-parse", "refs/heads/feature"]).unwrap(),
                evidence.head
            );
        }
    }
}

#[test]
fn all_three_ship_network_paths_bound_retries_and_report_the_agent() {
    let _guard = crate::ship::issues::tests::ENVIRONMENT.lock().unwrap();
    for path in ["initial-fetch", "recheck-fetch", "push"] {
        for error in ["Permission denied (publickey)", "Connection refused"] {
            let (temp, root) = fixture("true");
            let evidence = judge(&root, &offline()).unwrap();
            let counter = ssh_stub(&root, temp.path(), error, false);
            let (failure, waits) = git::remote::without_sleep(|| match path {
                "initial-fetch" => git::base(&root, "dev", true).unwrap_err(),
                "recheck-fetch" => recheck::before_push(&root, &evidence, None, &offline())
                    .err()
                    .unwrap()
                    .to_string(),
                "push" => crate::ship::push(&root, &evidence, None)
                    .unwrap_err()
                    .to_string(),
                _ => unreachable!(),
            });
            let publickey = error.contains("publickey");
            assert_eq!(waits.len(), usize::from(publickey), "{path}: {failure}");
            assert_eq!(
                fs::read_to_string(counter).unwrap().lines().count(),
                if publickey { 2 } else { 1 }
            );
            assert_eq!(
                failure.contains("served no key"),
                publickey,
                "{path}: {failure}"
            );
            if publickey {
                assert!(failure.contains("SSH_AUTH_SOCK"));
                assert!(failure.contains("GIT_SSH_COMMAND='ssh -i <key> -o IdentitiesOnly=yes'"));
            }
        }
    }
}

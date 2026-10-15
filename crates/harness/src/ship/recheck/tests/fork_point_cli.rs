use super::{fork_point::*, *};
use std::{io::Write, os::unix::fs::PermissionsExt, process::Command};

#[test]
fn the_ship_command_uses_the_parent_fork_point_or_refuses() {
    let _guard = crate::ship::issues::tests::ENVIRONMENT.lock().unwrap();
    for local in [true, false] {
        for expire in [false, true] {
            // Stop at the gate after catching up, before any reviewer or publication.
            let (temp, root, _, old_parent) =
                inherited(local, "echo boundary-gate >> ../gate.log; false");
            let moved = move_parent(&root, local, true);
            if expire {
                run(&root, &["reflog", "expire", "--expire=now", "--all"]);
            }
            let before = head(&root);
            let body = temp.path().join("body.md");
            fs::write(&body, "<!-- level: low -->\n## Why\n> \"replay only the child\" — the user\n## What\nRehearse the stacked replay boundary.\n").unwrap();
            let bin = temp.path().join("bin");
            fs::create_dir(&bin).unwrap();
            let real_git = std::env::split_paths(&std::env::var_os("PATH").unwrap())
                .map(|dir| dir.join("git"))
                .find(|file| file.is_file())
                .unwrap();
            let scripts = [
                (
                    "git",
                    format!(
                        "#!/bin/sh\nif [ \"$3 $4 $5\" = 'remote get-url origin' ]; then\n printf 'https://github.com/project/checkout.git\\n'\nelse\n exec '{}' \"$@\"\nfi\n",
                        real_git.display()
                    ),
                ),
                (
                    "gh",
                    format!(
                        r##"#!/bin/sh
case "$1 $2" in
  '--version ') echo 'gh version 2.100.0' ;;
  'pr list') case " $* " in
    *' --head parent '*) printf '%s\n' '{}' ;;
    *) : ;;
  esac ;;
  *) echo 'unexpected GitHub call' >&2; exit 99 ;;
esac
"##,
                        if local {
                            "[]"
                        } else {
                            r##"[{"number":12,"state":"OPEN","url":"https://github.com/project/checkout/pull/12","body":"# 🤖 For AI\n## Adversarial review","isCrossRepository":false}]"##
                        }
                    ),
                ),
            ];
            for (tool, script) in scripts {
                fs::write(bin.join(tool), script).unwrap();
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
                .args(["ship", "--title", "Ship: replay boundary", "--body-file"])
                .arg(body)
                .env("SHIP_BASE", "parent")
                .env(
                    "PATH",
                    format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
                )
                .env_remove("AOE_JUDGE_REV")
                .env_remove("SHIP_VIDEO")
                .env_remove("SHIP_TIER")
                .output()
                .unwrap();
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(!output.status.success(), "{stderr}");
            if expire {
                assert!(
                    stderr.contains("rebase the child onto parent yourself, then ship"),
                    "{stderr}"
                );
                assert_eq!(head(&root), before);
                assert_eq!(runs(&temp), 0);
                assert!(root.join("obsolete.txt").exists());
                assert!(!root.join("replacement.txt").exists());
            } else {
                assert!(stderr.contains("rebased onto"), "{stderr}");
                assert!(
                    stderr.contains("ship: FAIL;"),
                    "the deliberate gate stops publication: {stderr}"
                );
                assert!(contains(&root, &moved));
                assert!(!contains(&root, &old_parent));
                assert!(!root.join("obsolete.txt").exists());
                assert!(root.join("replacement.txt").exists());
                assert_eq!(
                    git(&root, &["rev-list", "--count", &format!("{moved}..HEAD")]),
                    "1"
                );
                assert_eq!(
                    git(&root, &["diff", "--name-only", &moved, "HEAD"]),
                    "child.txt"
                );
                assert_eq!(runs(&temp), 1);
            }
            assert_eq!(
                fs::read_to_string(root.join("child.txt")).unwrap(),
                "child\n"
            );
            assert_eq!(git(&root, &["status", "--porcelain"]), "");
            assert_eq!(remote_tip(&root, "feature"), "");
            writeln!(std::io::stdout(), "CLI rehearsal: local={local}, expired reflog={expire}; branch binary `aoe-harness ship --title 'Ship: replay boundary' --body-file <fixture>/body.md`, SHIP_BASE=parent, local Git origin, scripted GitHub\n{stderr}ASSERT: {}; child.txt present; clean tree; nothing pushed", if expire { "refused before gates; HEAD unchanged" } else { "only C replayed onto Q; obsolete.txt absent; replacement.txt present; one child commit" }).unwrap();
        }
    }
}

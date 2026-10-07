use super::{super::role::Role, Fixture, ROLES, allowed, denied};

#[test]
fn no_role_bypasses_hooks_merges_approves_or_moves_protected_branches() {
    let fixture = Fixture::new();
    for role in ROLES {
        for command in [
            "git commit --no-verify -m x",
            "git commit --no-ver -m x",
            "git commit -anm x",
            "git -c core.hooksPath=/dev/null commit -m x",
            "git --config-env=core.hooksPath=X commit -m x",
            "git push --no-verify origin feature",
            "git push origin dev",
            "git push origin HEAD:main",
            "git push origin +feature:refs/heads/dev",
            "git push origin feature:release/1.0",
            "git push --all origin",
            "git push --mirror origin",
            "git push origin --tags",
            "git push origin v1:refs/tags/v1",
            "git push origin --delete main",
            "git push origin :dev",
            "git push --receive-pack=evil origin feature",
            "git send-pack origin feature",
            "git update-ref refs/heads/dev HEAD",
            "git branch -f main HEAD",
            "git checkout -B dev",
            "git switch -C main",
            "git config core.hooksPath /tmp/hooks",
            "git config alias.ship '!git push origin dev'",
            "git config --global core.pager 'sh -c x'",
            "git config include.path /tmp/evil",
            "git config --edit",
            "git --git-dir=/tmp/x status",
            "gh pr merge 12 --squash",
            "gh pr review 12 --approve",
            "gh pr review 12 -a",
            "gh pr review 12 --appr",
            "gh pr update-branch 12",
            "gh api -X PUT repos/o/r/pulls/12/merge",
            "gh api repos/o/r/git/refs/heads/dev -f sha=abc -X PATCH",
            "gh api graphql -f query='mutation { mergePullRequest(input: {}) { clientMutationId } }'",
            "gh alias set ship 'pr merge'",
            "gh extension install owner/ext",
            "gh repo edit --default-branch main",
            "gh auth token",
            "xargs git push origin dev < /dev/null",
            "bash -c 'git push origin main'",
        ] {
            denied(&fixture, role, command);
        }
    }
}

#[test]
fn the_main_session_and_other_agents_ship_feature_branches() {
    let fixture = Fixture::new();
    for role in [Role::Main, Role::Other] {
        for command in [
            "git add -A && git commit -m 'Add claude hook'",
            "git push -u origin feature",
            "git push origin HEAD:refs/heads/harness/claude",
            "git push --force-with-lease origin feature",
            "git push -n origin feature",
            "gh pr create --base dev --title x --body y",
            "gh pr view 12 --json state",
            "gh api repos/o/r/pulls/12",
            "gh api graphql -f query='{ viewer { login } }'",
            "git config --get user.name",
            "git -C /tmp status",
            "git fetch origin dev",
        ] {
            allowed(&fixture, role, command);
        }
    }
    allowed(&fixture, Role::Main, "git config user.email me@example.com");
    allowed(
        &fixture,
        Role::Main,
        "git worktree add ../x -b x origin/dev",
    );
    allowed(
        &fixture,
        Role::Main,
        "gh api -X POST repos/o/r/issues/1/comments -f body=hi",
    );
    denied(
        &fixture,
        Role::Other,
        "gh api -X POST repos/o/r/issues/1/comments -f body=hi",
    );
    denied(
        &fixture,
        Role::Other,
        "git config user.email me@example.com",
    );
}

#[test]
fn duty_bound_agents_only_read_git_and_github() {
    let fixture = Fixture::new();
    for role in [Role::Tester, Role::Implementer, Role::Reviewer] {
        for command in [
            "git status --short",
            "git log --oneline -10",
            "git diff --stat HEAD~1",
            "git show HEAD:Makefile",
            "git branch --show-current",
            "git branch -a",
            "git config --get remote.origin.url",
            "git stash list",
            "gh pr view 12",
            "gh pr checks 12",
        ] {
            allowed(&fixture, role, command);
        }
        for command in [
            "git commit -m x",
            "git add -A",
            "git push -u origin feature",
            "git checkout -- crates/map/src/lib.rs",
            "git stash",
            "git reset --hard",
            "git branch feature-2",
            "git tag v1",
            "git config user.name x",
            "git diff --output=crates/map/src/lib.rs",
            "git apply fix.diff",
            "gh pr create --base dev",
            "gh pr comment 12 --body x",
        ] {
            denied(&fixture, role, command);
        }
    }
}

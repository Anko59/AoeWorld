use super::*;
use std::{fs, os::unix::fs::PermissionsExt, path::Path, process::Stdio};

const TOKEN: &str = "gho_fixture-token";

fn environment(command: &Command) -> Vec<(String, String)> {
    command
        .get_envs()
        .map(|(name, value)| {
            (
                name.to_string_lossy().into_owned(),
                value.unwrap_or_default().to_string_lossy().into_owned(),
            )
        })
        .collect()
}

fn line(command: &Command) -> Vec<String> {
    std::iter::once(command.get_program())
        .chain(command.get_args())
        .map(|word| word.to_string_lossy().into_owned())
        .collect()
}

#[test]
fn an_account_is_chosen_only_on_a_mac_and_only_when_named() {
    let named = |_: &str| Some("octo".to_owned());
    assert_eq!(selected_in(true, named).as_deref(), Some("octo"));
    assert_eq!(selected_in(false, named), None, "Linux keeps its own login");
    assert_eq!(selected_in(true, |_| Some(String::new())), None);
    assert_eq!(selected_in(true, |_| None), None);
}

#[test]
fn gh_gets_the_accounts_token_in_its_environment_and_never_in_its_arguments() {
    let mut command = gh_as(Some("octo"), |account| {
        assert_eq!(account, "octo");
        Ok(TOKEN.to_owned())
    })
    .unwrap();
    command.args(["pr", "list"]);
    assert_eq!(
        environment(&command),
        [("GH_TOKEN".to_owned(), TOKEN.to_owned())]
    );
    assert_eq!(line(&command), ["gh", "pr", "list"]);
    let plain = gh_as(None, |_| panic!("no account, no token")).unwrap();
    assert!(environment(&plain).is_empty());
    let refused = gh_as(Some("octo"), |_| Err("no such account".into())).unwrap_err();
    assert!(refused.contains("AOE_GH_ACCOUNT=octo"), "{refused}");
}

#[test]
fn git_is_given_a_helper_that_names_the_account_and_holds_no_token() {
    let mut command = Command::new("git");
    command.args(["push", "origin"]);
    git_as(&mut command, Some("octo"), 1);
    assert_eq!(line(&command), ["git", "push", "origin"]);
    let set = environment(&command);
    let value = |name: &str| set.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str());
    assert_eq!(
        value("GIT_CONFIG_COUNT"),
        Some("3"),
        "the caller's own is kept"
    );
    assert_eq!(value("GIT_CONFIG_KEY_0"), None);
    assert_eq!(
        value("GIT_CONFIG_VALUE_1"),
        Some(""),
        "other helpers are reset"
    );
    assert_eq!(value("AOE_GH_ACCOUNT"), Some("octo"));
    assert_eq!(value("GH_TOKEN"), None);
    let mut plain = Command::new("git");
    git_as(&mut plain, None, 0);
    assert!(environment(&plain).is_empty());
}

/// `git credential fill` for `host` with the account's helper and a `gh` that
/// answers with `gh`, where another helper and a prompt would also answer.
fn fill(home: &Path, host: &str, gh: &str) -> (bool, String) {
    let bin = home.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let recorded = format!("echo \"$*\" >>\"$HOME/gh\"\n{gh}");
    for (name, body) in [("gh", recorded.as_str()), ("askpass", "echo asked")] {
        fs::write(bin.join(name), format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(bin.join(name), fs::Permissions::from_mode(0o755)).unwrap();
    }
    fs::write(
        home.join(".gitconfig"),
        "[credential]\n\thelper = \"!f() { echo username=other; echo password=other; }; f\"\n",
    )
    .unwrap();
    fs::write(home.join("in"), format!("protocol=https\nhost={host}\n\n")).unwrap();
    let mut command = Command::new("git");
    git_as(&mut command, Some("octo"), 0);
    let output = command
        .args(["credential", "fill"])
        .current_dir(home)
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ASKPASS", bin.join("askpass"))
        .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
        .stdin(Stdio::from(fs::File::open(home.join("in")).unwrap()))
        .output()
        .unwrap();
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
    )
}

#[test]
fn git_over_https_authenticates_to_github_as_the_account_and_elsewhere_as_before() {
    let temp = tempfile::tempdir().unwrap();
    let (ok, text) = fill(temp.path(), "github.com", &format!("echo {TOKEN}"));
    assert!(ok, "{text}");
    assert!(text.contains("username=octo\n"), "{text}");
    assert!(text.contains(&format!("password={TOKEN}\n")), "{text}");
    let asked = fs::read_to_string(temp.path().join("gh")).unwrap();
    assert_eq!(asked, "auth token --hostname github.com --user octo\n");
    let (ok, text) = fill(temp.path(), "example.com", &format!("echo {TOKEN}"));
    assert!(ok && text.contains("password=other\n"), "{text}");
}

#[test]
fn an_unknown_account_stops_git_instead_of_using_another_login() {
    let temp = tempfile::tempdir().unwrap();
    let (ok, text) = fill(temp.path(), "github.com", "exit 1");
    assert!(!ok && !text.contains("password"), "{text}");
}

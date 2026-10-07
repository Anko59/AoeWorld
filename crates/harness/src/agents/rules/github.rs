//! GitHub CLI rules: an agent never merges, approves or rewrites GitHub state.
use super::SHIP;
use crate::agents::{
    args::{self, Spec},
    context::{Context, Verdict},
    shell::Word,
};

pub(crate) fn gh(context: &Context, rest: &[Word]) -> Verdict {
    let agent = context.role.is_agent();
    let words: Vec<&str> = rest.iter().map(|w| w.text.as_str()).collect();
    let refuse = |what: &str| {
        Err(format!(
            "{what} belongs to a person: an agent never merges, approves or rewrites GitHub state; {SHIP}"
        ))
    };
    // Flags may precede the verb (`gh pr -R o/r merge`): drop them and the
    // values they take, and fold gh's aliases onto the verbs the rules name.
    let mut positional: Vec<&str> = Vec::new();
    let mut skip = false;
    for word in words.iter().copied() {
        if skip {
            skip = false;
        } else if matches!(word, "-R" | "--repo" | "--hostname") {
            skip = true;
        } else if !word.starts_with('-') {
            positional.push(match word {
                "new" => "create",
                other => other,
            });
        }
    }
    match positional.as_slice() {
        ["pr", "merge", ..] => return refuse("`gh pr merge`"),
        ["pr", "create", ..] => {
            return Err(format!("pull requests are opened by `make ship`; {SHIP}"));
        }
        ["pr", "update-branch", ..] => return refuse("`gh pr update-branch`"),
        ["pr", "review", ..] => {
            let parsed = args::split(
                rest,
                &Spec {
                    short: "bF",
                    long: &["body", "body-file"],
                    stop_at_positional: false,
                },
            );
            if parsed.has(&["-a", "--approve"])
                || parsed.has_long("--approve", 4)
                || context.role.duty_bound()
            {
                return refuse("approving a pull request");
            }
        }
        [
            "repo",
            "delete" | "edit" | "rename" | "archive" | "sync" | "set-default",
            ..,
        ] => return refuse("changing repository settings"),
        ["ruleset" | "secret" | "variable", verb, ..]
            if !matches!(*verb, "list" | "view" | "check") =>
        {
            return refuse("changing rulesets, secrets or variables");
        }
        ["release", verb, ..] if !matches!(*verb, "list" | "view" | "download") => {
            return refuse("`gh release` changes");
        }
        ["alias", verb, ..] if !matches!(*verb, "list") => {
            return Err("gh aliases run commands; they are not changed by a Claude session".into());
        }
        ["extension" | "ext", verb, ..] if !matches!(*verb, "list" | "search" | "browse") => {
            return Err(
                "gh extensions run code; they are not installed by a Claude session".into(),
            );
        }
        ["config", "set" | "clear-cache", ..] => {
            return Err("gh configuration is not changed by a Claude session".into());
        }
        ["auth", verb, ..] if *verb != "status" => return refuse("`gh auth`"),
        ["api", ..] => return api(agent, rest),
        _ => {}
    }
    if context.role.duty_bound() {
        let read = matches!(
            positional.as_slice(),
            [_, "view" | "list" | "diff" | "checks" | "status", ..]
                | ["search", ..]
                | ["browse", ..]
        );
        if !read {
            return Err(format!(
                "the {} only reads GitHub (`gh … view/list/diff/checks`)",
                context.role.name()
            ));
        }
    }
    if agent
        && matches!(
            positional.as_slice(),
            ["pr", "checkout", ..] | ["repo", "clone", ..] | ["workflow" | "run" | "cache", ..]
        )
        && !matches!(positional.get(1), Some(&"view" | &"list" | &"watch"))
    {
        return Err("agents do not change branches or run workflows through gh".into());
    }
    Ok(())
}

fn api(agent: bool, rest: &[Word]) -> Verdict {
    let parsed = args::split(
        &rest[1..],
        &Spec {
            short: "XfFHpqt",
            long: &[
                "method",
                "field",
                "raw-field",
                "header",
                "input",
                "jq",
                "template",
                "preview",
                "hostname",
                "cache",
            ],
            stop_at_positional: false,
        },
    );
    let method = parsed
        .values(&["-X", "--method"])
        .last()
        .map(|w| w.text.to_ascii_uppercase());
    let fields = parsed.has(&["-f", "-F", "--field", "--raw-field", "--input"]);
    let endpoint = parsed
        .positionals
        .first()
        .map(|w| w.text.to_ascii_lowercase())
        .unwrap_or_default();
    let graphql = endpoint == "graphql";
    let mutation = parsed
        .values(&["-f", "-F", "--field", "--raw-field"])
        .any(|w| w.text.to_ascii_lowercase().contains("mutation"));
    let writes = match method.as_deref() {
        Some("GET" | "HEAD") => false,
        Some(_) => true,
        None => fields && !graphql,
    } || (graphql && (mutation || parsed.has(&["--input"])));
    if !writes {
        return Ok(());
    }
    if agent {
        return Err("agents call `gh api` with GET only".into());
    }
    const SENSITIVE: &[&str] = &[
        "/merge",
        "git/refs",
        "/contents/",
        "/branches",
        "/reviews",
        "/statuses",
        "/check-runs",
        "/check-suites",
        "/rulesets",
        "/protection",
        "/collaborators",
        "/hooks",
        "/actions/secrets",
    ];
    if graphql || SENSITIVE.iter().any(|s| endpoint.contains(s)) {
        return Err("`gh api` writes to merges, refs, contents, branches, reviews or checks belong to a person; use the dedicated `gh pr` commands".into());
    }
    Ok(())
}

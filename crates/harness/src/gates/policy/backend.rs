//! Protected-source observation and private materialization, not execution authority.
use super::*;
use crate::{
    gates::{
        runner::evidence::PrivateOutput,
        scopes::{Kind, Snapshot},
    },
    process::{self, Cancellation, CaptureExit},
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tempfile::TempDir;

pub(super) struct Resolved {
    // Snapshot drops before its owning external fetch repository.
    pub(super) snapshot: Snapshot,
    pub(super) identity: SourceIdentity,
    pub(super) owner: TempDir,
}

pub(super) fn resolve(
    anchor: &Anchor,
    output: &PrivateOutput,
    cancellation: &Cancellation,
) -> Result<Resolved> {
    // Even internal callers cannot bypass validated URL/branch/identity syntax.
    let anchor = Anchor::parse(&serde_json::to_vec(anchor)?)?;
    let owner = tempfile::Builder::new()
        .prefix("policy-fetch-")
        .tempdir_in(output.directory())?;
    let root = owner.path().join("repository");
    let home = owner.path().join("home");
    fs::create_dir(&root)?;
    fs::create_dir(&home)?;
    let repository = api(
        &root,
        &format!("repos/{}", anchor.repository),
        "policy-api-repository.log",
        output,
        cancellation,
    )?;
    let branch = api(
        &root,
        &format!("repos/{}/branches/dev", anchor.repository),
        "policy-api-branch.log",
        output,
        cancellation,
    )?;
    let protection = api(
        &root,
        &format!("repos/{}/branches/dev/protection", anchor.repository),
        "policy-api-protection.log",
        output,
        cancellation,
    )?;
    let observed = observed(&anchor, &repository, &branch, &protection)?;
    let advertised = git(
        &root,
        &home,
        &["ls-remote", "--refs", &anchor.remote_url, "refs/heads/dev"],
        "policy-ls-remote.log",
        output,
        cancellation,
        Duration::from_secs(60),
    )?;
    let advertised = advertised_commit(&advertised)?;
    // Reject changed base BEFORE fetching, never silently adopt the newer SHA.
    observed.resolve(&anchor, &advertised, &observed.commit)?;
    git(
        &root,
        &home,
        &["init", "--quiet", "--template="],
        "policy-git-init.log",
        output,
        cancellation,
        Duration::from_secs(5),
    )?;
    git(
        &root,
        &home,
        &[
            "fetch",
            "--no-tags",
            "--no-recurse-submodules",
            "--no-write-fetch-head",
            "--depth=1",
            &anchor.remote_url,
            &observed.commit,
        ],
        "policy-fetch.log",
        output,
        cancellation,
        Duration::from_secs(60),
    )?;
    let kind = git(
        &root,
        &home,
        &["cat-file", "-t", &observed.commit],
        "policy-object-type.log",
        output,
        cancellation,
        Duration::from_secs(5),
    )?;
    if line(&kind)? != "commit" {
        return Err("fetched protected object is not a commit".into());
    }
    let tree = git(
        &root,
        &home,
        &[
            "rev-parse",
            "--verify",
            &format!("{}^{{tree}}", observed.commit),
        ],
        "policy-tree.log",
        output,
        cancellation,
        Duration::from_secs(5),
    )?;
    let tree = line(&tree)?;
    let identity = observed.resolve(&anchor, &advertised, &tree)?;
    git(
        &root,
        &home,
        &["update-ref", "--no-deref", "HEAD", &identity.commit],
        "policy-detach.log",
        output,
        cancellation,
        Duration::from_secs(5),
    )?;
    git(
        &root,
        &home,
        &["read-tree", &identity.commit],
        "policy-index.log",
        output,
        cancellation,
        Duration::from_secs(5),
    )?;
    materialize(owner, identity)
}

fn materialize(owner: TempDir, identity: SourceIdentity) -> Result<Resolved> {
    let snapshot = Snapshot::prepare_independent(
        &owner.path().join("repository"),
        Kind::Commit(identity.commit.clone()),
    )?;
    if snapshot.identity.tree.as_deref() != Some(&identity.tree) {
        return Err("protected snapshot tree differs from fetched identity".into());
    }
    snapshot.run_checked(|_| Ok(()))?;
    Ok(Resolved {
        snapshot,
        identity,
        owner,
    })
}

fn api(
    root: &Path,
    endpoint: &str,
    name: &str,
    output: &PrivateOutput,
    cancellation: &Cancellation,
) -> Result<serde_json::Value> {
    // Credentials belong to this resolver client only. The Git child uses env -i.
    let bytes = capture(
        root,
        "gh",
        &[
            "api",
            "--hostname",
            "github.com",
            "--method",
            "GET",
            endpoint,
        ],
        name,
        output,
        cancellation,
        Duration::from_secs(60),
    )?;
    Ok(serde_json::from_slice(&bytes)?)
}
fn capture(
    root: &Path,
    program: &str,
    args: &[&str],
    name: &str,
    output: &PrivateOutput,
    cancellation: &Cancellation,
    deadline: Duration,
) -> Result<Vec<u8>> {
    let captured = process::capture_in(root, program, args, &[], deadline, cancellation);
    let mut log = b"--- stdout ---\n".to_vec();
    log.extend(&captured.stdout);
    log.extend(b"\n--- stderr ---\n");
    log.extend(&captured.stderr);
    output.atomic(name, &log)?;
    if captured.truncated {
        return Err(format!("protected resolver output incomplete/truncated: {name}").into());
    }
    match captured.exit {
        CaptureExit::Success => Ok(captured.stdout),
        CaptureExit::Failed(code) => {
            Err(format!("protected resolver command failed ({code:?}); retained {name}").into())
        }
        CaptureExit::Deadline => {
            Err(format!("protected resolver deadline; retained {name}").into())
        }
        CaptureExit::Cancelled => {
            Err(format!("protected resolver cancelled; retained {name}").into())
        }
        CaptureExit::Start(error) | CaptureExit::Monitor(error) => {
            Err(format!("protected resolver process error: {error}; retained {name}").into())
        }
    }
}
fn git(
    root: &Path,
    home: &Path,
    args: &[&str],
    name: &str,
    output: &PrivateOutput,
    cancellation: &Cancellation,
    deadline: Duration,
) -> Result<Vec<u8>> {
    let arguments = git_arguments(home, args)?;
    let arguments: Vec<_> = arguments.iter().map(String::as_str).collect();
    capture(
        root,
        "env",
        &arguments,
        name,
        output,
        cancellation,
        deadline,
    )
}
fn git_arguments(home: &Path, args: &[&str]) -> Result<Vec<String>> {
    let path = std::env::var("PATH")?;
    let home = home.to_str().ok_or("private resolver HOME must be UTF-8")?;
    let mut argv = vec![
        "-i".into(),
        format!("PATH={path}"),
        format!("HOME={home}"),
        format!("XDG_CONFIG_HOME={home}/.config"),
    ];
    argv.extend(
        [
            "LANG=C.UTF-8",
            "GIT_CONFIG_GLOBAL=/dev/null",
            "GIT_CONFIG_SYSTEM=/dev/null",
            "GIT_CONFIG_NOSYSTEM=1",
            "GIT_NO_REPLACE_OBJECTS=1",
            "GIT_ATTR_NOSYSTEM=1",
            "GIT_OPTIONAL_LOCKS=0",
            "GIT_TERMINAL_PROMPT=0",
            "git",
        ]
        .map(str::to_owned),
    );
    for value in [
        "core.hooksPath=/dev/null",
        "credential.helper=",
        "protocol.allow=never",
        "protocol.https.allow=always",
        "protocol.file.allow=never",
        "protocol.ext.allow=never",
        "http.followRedirects=false",
        "fetch.fsckObjects=true",
        "transfer.fsckObjects=true",
    ] {
        argv.extend(["-c".into(), value.into()]);
    }
    argv.extend(args.iter().map(|arg| (*arg).to_owned()));
    Ok(argv)
}
fn line(bytes: &[u8]) -> Result<String> {
    let text = std::str::from_utf8(bytes)?
        .strip_suffix('\n')
        .unwrap_or(std::str::from_utf8(bytes)?);
    if text.is_empty() || text.contains(['\n', '\r', '\0']) {
        return Err("expected one complete resolver output line".into());
    }
    Ok(text.to_owned())
}
fn advertised_commit(bytes: &[u8]) -> Result<String> {
    let record = line(bytes)?;
    let (commit, reference) = record
        .split_once('\t')
        .ok_or("malformed advertised protected ref")?;
    if reference != "refs/heads/dev" {
        return Err("advertised unexpected protected ref".into());
    }
    full_oid(commit)?;
    Ok(commit.to_owned())
}
fn observed(
    anchor: &Anchor,
    repository: &serde_json::Value,
    branch: &serde_json::Value,
    protection: &serde_json::Value,
) -> Result<ObservedBranch> {
    let text = |value: &serde_json::Value, field: &str| -> Result<String> {
        Ok(value
            .get(field)
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("GitHub observation missing string {field}"))?
            .to_owned())
    };
    let required = protection
        .get("required_status_checks")
        .filter(|value| value.is_object())
        .ok_or("required status protection unavailable")?;
    let strict = required
        .get("strict")
        .and_then(serde_json::Value::as_bool)
        .ok_or("strict protection observation unavailable")?;
    let mut contexts = BTreeSet::new();
    if let Some(values) = required.get("contexts") {
        for value in values.as_array().ok_or("malformed protection contexts")? {
            contexts.insert(
                value
                    .as_str()
                    .ok_or("malformed protection context")?
                    .to_owned(),
            );
        }
    }
    if let Some(values) = required.get("checks") {
        for value in values.as_array().ok_or("malformed protection checks")? {
            contexts.insert(text(value, "context")?);
        }
    }
    let observed = ObservedBranch {
        repository: text(repository, "full_name")?,
        repository_id: repository
            .get("id")
            .and_then(serde_json::Value::as_u64)
            .ok_or("numeric repository identity unavailable")?,
        branch: text(branch, "name")?,
        protected: branch
            .get("protected")
            .and_then(serde_json::Value::as_bool)
            .ok_or("protected branch observation unavailable")?,
        commit: text(
            branch.get("commit").ok_or("branch commit unavailable")?,
            "sha",
        )?,
        required_contexts: contexts,
        strict,
        observed_at_unix_s: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
    };
    // Pre-fetch validation; actual tree is resolved ONLY from fetched Git bytes.
    observed.resolve(anchor, &observed.commit, &observed.commit)?;
    Ok(observed)
}

#[cfg(test)]
mod tests;

//! Read-only release PR validation against published dev artifacts.
use crate::{
    release,
    release_publish::{self, PublishedManifest},
};
use std::{collections::BTreeMap, env, error::Error, fs, io::Write, path::Path, process::Command};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const BOOTSTRAP_MAIN: &str = "fdde33b5853c4fec40b6d19ce8d78df3a2af5d20";

trait Runtime {
    fn output(&self, program: &str, args: &[&str]) -> Result<String>;
    fn checked(&self, program: &str, args: &[&str]) -> Result<()>;
}

struct RealRuntime;

impl Runtime for RealRuntime {
    fn output(&self, program: &str, args: &[&str]) -> Result<String> {
        let result = Command::new(program).args(args).output()?;
        if !result.status.success() {
            return Err(format!("{program} {args:?} failed").into());
        }
        Ok(String::from_utf8(result.stdout)?.trim().to_owned())
    }

    fn checked(&self, program: &str, args: &[&str]) -> Result<()> {
        let status = Command::new(program).args(args).status()?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("{program} {args:?} failed: {status}").into())
        }
    }
}

fn source_with<R: Runtime>(runtime: &R, branch: &str) -> Result<(String, String)> {
    let source = branch
        .strip_prefix("release/")
        .ok_or("release branch must be named release/<dev SHA>")?;
    if source.len() != 40 || !source.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("release branch has an invalid dev SHA".into());
    }
    let tree = runtime.output("git", &["rev-parse", "HEAD^{tree}"])?;
    let dev_tree = runtime.output("git", &["rev-parse", &format!("{source}^{{tree}}")])?;
    if tree != dev_tree {
        return Err("release branch tree differs from verified dev source".into());
    }
    runtime.checked(
        "git",
        &["merge-base", "--is-ancestor", source, "origin/dev"],
    )?;
    Ok((source.to_owned(), tree))
}

pub fn source() -> Result<()> {
    let branch = env::var("AOE_RELEASE_BRANCH")?;
    let (source, _) = source_with(&RealRuntime, &branch)?;
    if let Ok(path) = env::var("GITHUB_OUTPUT") {
        let mut output = fs::OpenOptions::new().append(true).open(path)?;
        writeln!(output, "source_sha={source}")?;
    }
    println!("release branch carries verified dev tree {source}");
    Ok(())
}

fn main_source_with<R: Runtime>(runtime: &R) -> Result<String> {
    if runtime.output("git", &["branch", "--show-current"])? != "main" {
        return Err("artifact promotion requires the main branch".into());
    }
    let tree = runtime.output("git", &["rev-parse", "HEAD^{tree}"])?;
    let history = runtime.output("git", &["log", "origin/dev", "--format=%H"])?;
    let mut matches = Vec::new();
    for revision in history.lines() {
        if revision.len() != 40 || !revision.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("invalid dev history revision".into());
        }
        if runtime.output("git", &["rev-parse", &format!("{revision}^{{tree}}")])? == tree {
            matches.push(revision.to_owned());
        }
    }
    if matches.len() != 1 {
        return Err("main tree must match exactly one verified dev revision".into());
    }
    Ok(matches.remove(0))
}

pub fn main_source() -> Result<()> {
    let source = main_source_with(&RealRuntime)?;
    if let Ok(path) = env::var("GITHUB_OUTPUT") {
        let mut output = fs::OpenOptions::new().append(true).open(path)?;
        writeln!(output, "source_sha={source}")?;
    }
    println!("main tree matches verified dev revision {source}");
    Ok(())
}

fn valid_image(reference: &str, expected_name: &str) -> bool {
    reference
        .strip_prefix(&format!("{expected_name}@"))
        .is_some_and(release_publish::valid_sha256)
}

fn verify_checksums(directory: &Path) -> Result<()> {
    let expected = [
        "published.json",
        "bundle.tar",
        "server.spdx.json",
        "browser.spdx.json",
        "bundle.spdx.json",
        "e2e.json",
        "perf.json",
    ];
    let lines = fs::read_to_string(directory.join("checksums.txt"))?;
    let mut seen = BTreeMap::new();
    for line in lines.lines() {
        let (hash, name) = line
            .split_once("  ")
            .ok_or("invalid release checksum line")?;
        if !expected.contains(&name) || seen.insert(name, hash).is_some() {
            return Err("unknown or duplicate release checksum".into());
        }
    }
    if seen.len() != expected.len() {
        return Err("release checksum inventory is incomplete".into());
    }
    for name in expected {
        let actual = format!(
            "blake3:{}",
            blake3::hash(&fs::read(directory.join(name))?).to_hex()
        );
        if seen.get(name).copied() != Some(actual.as_str()) {
            return Err(format!("release checksum mismatch: {name}").into());
        }
    }
    Ok(())
}

fn verify_files(
    directory: &Path,
    source: &str,
    tree: &str,
    registry: &str,
) -> Result<PublishedManifest> {
    let published: PublishedManifest =
        serde_json::from_slice(&fs::read(directory.join("published.json"))?)?;
    if published.version != 1
        || published.source_commit != source
        || published.source_tree != tree
        || !valid_image(&published.server_image, &format!("{registry}-server"))
        || !valid_image(&published.browser_image, &format!("{registry}-browser"))
        || published.protocol_version != aoe_protocol::VERSION
        || published.asset_pack_version != 1
        || !published.rustc.starts_with("rustc 1.93.1 ")
    {
        return Err("published release identity or format is incompatible".into());
    }
    let archive_hash = format!(
        "blake3:{}",
        blake3::hash(&fs::read(directory.join("bundle.tar"))?).to_hex()
    );
    if published.bundle_archive_hash != archive_hash
        || published.sbom_hashes != release_publish::sbom_hashes(directory)?
        || published.e2e_report_hash
            != release::evidence(&directory.join("e2e.json"), source, "result")?
        || published.perf_report_hash
            != release::evidence(&directory.join("perf.json"), source, "verdict")?
    {
        return Err("published release artifact or evidence mismatch".into());
    }
    verify_checksums(directory)?;
    Ok(published)
}

fn pull_and_verify<R: Runtime>(runtime: &R, published: &PublishedManifest) -> Result<()> {
    for reference in [&published.server_image, &published.browser_image] {
        runtime.checked("docker", &["pull", reference])?;
        let actual = runtime.output(
            "docker",
            &[
                "image",
                "inspect",
                "--format",
                "{{json .RepoDigests}}",
                reference,
            ],
        )?;
        let digests: Vec<String> = serde_json::from_str(&actual)?;
        if !digests.contains(reference) {
            return Err("pulled image does not retain the published digest".into());
        }
    }
    Ok(())
}

fn report(path: &Path, published: &PublishedManifest, verdict: &str) -> Result<()> {
    let body = serde_json::json!({
        "version": 1,
        "source_commit": published.source_commit,
        "source_tree": published.source_tree,
        "server_image": published.server_image,
        "browser_image": published.browser_image,
        "verdict": verdict
    });
    fs::create_dir_all(path.parent().ok_or("release report has no parent")?)?;
    fs::write(path, serde_json::to_vec_pretty(&body)?)?;
    Ok(())
}

fn candidate_with<R: Runtime>(
    runtime: &R,
    branch: &str,
    registry: &str,
    root: &Path,
) -> Result<(String, PublishedManifest)> {
    let (source, tree) = source_with(runtime, branch)?;
    let published = verify_files(&root.join(&source), &source, &tree, registry)?;
    Ok((source, published))
}

fn verify_with<R: Runtime>(
    runtime: &R,
    branch: &str,
    expected_source: &str,
    registry: &str,
    root: &Path,
    github_output: Option<&Path>,
) -> Result<()> {
    let (source, published) = candidate_with(runtime, branch, registry, root)?;
    if expected_source != source {
        return Err("release source output does not match branch".into());
    }
    pull_and_verify(runtime, &published)?;
    let directory = root.join(&source);
    report(&directory.join("verification.json"), &published, "PASS")?;
    if let Some(path) = github_output {
        let mut output = fs::OpenOptions::new().append(true).open(path)?;
        writeln!(output, "server_image={}", published.server_image)?;
        writeln!(output, "browser_image={}", published.browser_image)?;
    }
    println!("verified published digests for {source}");
    Ok(())
}

pub fn verify() -> Result<()> {
    let registry = release_publish::valid_registry(&env::var("GITHUB_REPOSITORY")?)?;
    release_publish::login(&env::var("GITHUB_TOKEN")?, &env::var("GITHUB_ACTOR")?)?;
    verify_with(
        &RealRuntime,
        &env::var("AOE_RELEASE_BRANCH")?,
        &env::var("AOE_RELEASE_SOURCE_SHA")?,
        &registry,
        Path::new("reports/release"),
        env::var("GITHUB_OUTPUT").ok().as_deref().map(Path::new),
    )
}

fn rehearse_with<R: Runtime>(
    runtime: &R,
    branch: &str,
    registry: &str,
    root: &Path,
    previous_path: &Path,
    stack: impl FnOnce(&PublishedManifest, &PublishedManifest) -> Result<()>,
) -> Result<()> {
    let (source, candidate) = candidate_with(runtime, branch, registry, root)?;
    let candidate_dir = root.join(&source);
    let previous_dir = previous_path
        .parent()
        .ok_or("previous manifest has no directory")?;
    let previous_raw: PublishedManifest = serde_json::from_slice(&fs::read(previous_path)?)?;
    if previous_raw.source_commit.len() != 40
        || !previous_raw
            .source_commit
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("previous release has invalid source commit".into());
    }
    let previous_tree = runtime.output(
        "git",
        &[
            "rev-parse",
            &format!("{}^{{tree}}", previous_raw.source_commit),
        ],
    )?;
    let previous = verify_files(
        previous_dir,
        &previous_raw.source_commit,
        &previous_tree,
        registry,
    )?;
    pull_and_verify(runtime, &candidate)?;
    pull_and_verify(runtime, &previous)?;
    stack(&candidate, &previous)?;
    let result = serde_json::json!({
        "version": 1,
        "candidate": candidate.source_commit,
        "previous": previous.source_commit,
        "candidate_server": candidate.server_image,
        "candidate_browser": candidate.browser_image,
        "previous_server": previous.server_image,
        "previous_browser": previous.browser_image,
        "verdict": "PASS"
    });
    fs::write(
        candidate_dir.join("promotion.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    println!("published promotion and rollback rehearsal passed");
    Ok(())
}

pub fn rehearse() -> Result<()> {
    let registry = release_publish::valid_registry(&env::var("GITHUB_REPOSITORY")?)?;
    release_publish::login(&env::var("GITHUB_TOKEN")?, &env::var("GITHUB_ACTOR")?)?;
    rehearse_with(
        &RealRuntime,
        &env::var("AOE_RELEASE_BRANCH")?,
        &registry,
        Path::new("reports/release"),
        Path::new(&env::var("AOE_PREVIOUS_MANIFEST")?),
        |candidate, previous| {
            crate::release_stack::rehearse_references(
                &candidate.source_commit,
                &candidate.server_image,
                &candidate.browser_image,
                &previous.source_commit,
                &previous.server_image,
                &previous.browser_image,
            )
        },
    )
}

fn smoke_with<R: Runtime>(
    runtime: &R,
    branch: &str,
    registry: &str,
    root: &Path,
    stack: impl FnOnce(&PublishedManifest) -> Result<()>,
) -> Result<()> {
    if runtime.output("git", &["rev-parse", "origin/main"])? != BOOTSTRAP_MAIN {
        return Err("a promoted release exists; rollback rehearsal is required".into());
    }
    let (source, candidate) = candidate_with(runtime, branch, registry, root)?;
    let directory = root.join(&source);
    pull_and_verify(runtime, &candidate)?;
    stack(&candidate)?;
    let result = serde_json::json!({
        "version": 1,
        "candidate": candidate.source_commit,
        "server_image": candidate.server_image,
        "browser_image": candidate.browser_image,
        "rollback_rehearsed": false,
        "verdict": "BOOTSTRAP"
    });
    fs::write(
        directory.join("promotion.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    println!("initial published candidate started; no previous release exists for rollback");
    Ok(())
}

pub fn smoke() -> Result<()> {
    let registry = release_publish::valid_registry(&env::var("GITHUB_REPOSITORY")?)?;
    release_publish::login(&env::var("GITHUB_TOKEN")?, &env::var("GITHUB_ACTOR")?)?;
    smoke_with(
        &RealRuntime,
        &env::var("AOE_RELEASE_BRANCH")?,
        &registry,
        Path::new("reports/release"),
        |candidate| {
            crate::release_stack::smoke_reference(
                &candidate.source_commit,
                &candidate.server_image,
                &candidate.browser_image,
            )
        },
    )
}

#[cfg(test)]
mod tests;

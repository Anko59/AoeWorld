//! Bind verified local release artifacts to immutable registry references.
use crate::release::{self, Manifest};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    env,
    error::Error,
    fs,
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublishedManifest {
    pub version: u16,
    pub source_commit: String,
    pub source_tree: String,
    pub server_image: String,
    pub browser_image: String,
    pub bundle_hash: String,
    pub bundle_archive_hash: String,
    pub protocol_version: u16,
    pub asset_pack_version: u32,
    pub rustc: String,
    pub e2e_report_hash: String,
    pub perf_report_hash: String,
    pub sbom_hashes: BTreeMap<String, String>,
}

trait Runtime {
    fn checked(&self, program: &str, args: &[&str]) -> Result<()>;
    fn output(&self, program: &str, args: &[&str]) -> Result<String>;
}

struct RealRuntime;

impl Runtime for RealRuntime {
    fn checked(&self, program: &str, args: &[&str]) -> Result<()> {
        let status = Command::new(program).args(args).status()?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("{program} {args:?} failed: {status}").into())
        }
    }

    fn output(&self, program: &str, args: &[&str]) -> Result<String> {
        let output = Command::new(program).args(args).output()?;
        if !output.status.success() {
            return Err(format!(
                "{program} {args:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            )
            .into());
        }
        Ok(String::from_utf8(output.stdout)?.trim().to_owned())
    }
}

fn digest(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

pub(crate) fn valid_registry(repository: &str) -> Result<String> {
    let normalized = repository.to_ascii_lowercase();
    let Some((owner, name)) = normalized.split_once('/') else {
        return Err("GITHUB_REPOSITORY must be owner/repository".into());
    };
    if [owner, name].iter().any(|part| {
        part.is_empty()
            || part
                .bytes()
                .any(|byte| !byte.is_ascii_lowercase() && !byte.is_ascii_digit() && byte != b'-')
    }) {
        return Err("invalid registry repository name".into());
    }
    Ok(format!("ghcr.io/{owner}/{name}"))
}

pub(crate) fn valid_sha256(value: &str) -> bool {
    value
        .strip_prefix("sha256:")
        .is_some_and(|hash| hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

fn repo_digest(output: &str, name: &str) -> Result<String> {
    let refs: Vec<String> = serde_json::from_str(output)?;
    let prefix = format!("{name}@");
    let mut matches = refs
        .into_iter()
        .filter(|reference| reference.starts_with(&prefix))
        .filter(|reference| valid_sha256(&reference[prefix.len()..]));
    let result = matches
        .next()
        .ok_or("pushed image has no registry digest")?;
    if matches.next().is_some() {
        return Err("pushed image has multiple registry digests".into());
    }
    Ok(result)
}

pub(crate) fn sbom_hashes(directory: &Path) -> Result<BTreeMap<String, String>> {
    let mut result = BTreeMap::new();
    for name in ["server.spdx.json", "browser.spdx.json", "bundle.spdx.json"] {
        let bytes = fs::read(directory.join(name))?;
        let document: Value = serde_json::from_slice(&bytes)?;
        if !document["spdxVersion"]
            .as_str()
            .is_some_and(|version| version.starts_with("SPDX-2."))
        {
            return Err(format!("{name} is not an SPDX JSON SBOM").into());
        }
        result.insert(name.to_owned(), digest(&bytes));
    }
    Ok(result)
}

fn push_with<R: Runtime>(
    runtime: &R,
    local: &Manifest,
    directory: &Path,
    registry: &str,
) -> Result<PublishedManifest> {
    let sboms = sbom_hashes(directory)?;
    let archive = directory.join("bundle.tar");
    runtime.checked(
        "tar",
        &[
            "-C",
            directory
                .join("bundle")
                .to_str()
                .ok_or("bundle path is not UTF-8")?,
            "-cf",
            archive.to_str().ok_or("archive path is not UTF-8")?,
            ".",
        ],
    )?;
    let archive_hash = digest(&fs::read(&archive)?);
    let mut refs = Vec::new();
    for (kind, image_id) in [
        ("server", &local.server_image_id),
        ("browser", &local.browser_image_id),
    ] {
        let name = format!("{registry}-{kind}");
        let tag = format!("{name}:{}", local.source_commit);
        runtime.checked("docker", &["tag", image_id, &tag])?;
        runtime.checked("docker", &["push", &tag])?;
        let digests = runtime.output(
            "docker",
            &[
                "image",
                "inspect",
                "--format",
                "{{json .RepoDigests}}",
                &tag,
            ],
        )?;
        refs.push(repo_digest(&digests, &name)?);
    }
    let server_image = refs.first().cloned().ok_or("server digest missing")?;
    let browser_image = refs.get(1).cloned().ok_or("browser digest missing")?;
    Ok(PublishedManifest {
        version: 1,
        source_commit: local.source_commit.clone(),
        source_tree: local.source_tree.clone(),
        server_image,
        browser_image,
        bundle_hash: local.bundle_hash.clone(),
        bundle_archive_hash: archive_hash,
        protocol_version: local.protocol_version,
        asset_pack_version: local.asset_pack_version,
        rustc: local.rustc.clone(),
        e2e_report_hash: local.e2e_report_hash.clone(),
        perf_report_hash: local.perf_report_hash.clone(),
        sbom_hashes: sboms,
    })
}

pub(crate) fn login(token: &str, actor: &str) -> Result<()> {
    if let Ok(directory) = env::var("DOCKER_CONFIG") {
        fs::create_dir_all(directory)?;
    }
    let mut child = Command::new("docker")
        .args(["login", "ghcr.io", "-u", actor, "--password-stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()?;
    child
        .stdin
        .take()
        .ok_or("docker login has no stdin")?
        .write_all(token.as_bytes())?;
    if !child.wait()?.success() {
        return Err("GHCR login failed".into());
    }
    Ok(())
}

fn output(path: Option<&Path>, name: &str, value: &str) -> Result<()> {
    if let Some(path) = path {
        let mut file = fs::OpenOptions::new().append(true).open(path)?;
        writeln!(file, "{name}={value}")?;
    }
    Ok(())
}

fn validate_context(reference: &str, revision: &str, local: &Manifest) -> Result<()> {
    if reference != "refs/heads/dev" {
        return Err("published artifacts require a verified dev push".into());
    }
    if local.source_commit != revision {
        return Err("release manifest does not match the dev push".into());
    }
    Ok(())
}

fn finalize(
    directory: &Path,
    published: &PublishedManifest,
    github_output: Option<&Path>,
) -> Result<()> {
    let manifest = directory.join("published.json");
    fs::write(&manifest, serde_json::to_vec_pretty(published)?)?;
    let mut checksums = String::new();
    for name in [
        "published.json",
        "bundle.tar",
        "server.spdx.json",
        "browser.spdx.json",
        "bundle.spdx.json",
        "e2e.json",
        "perf.json",
    ] {
        checksums.push_str(&format!(
            "{}  {name}\n",
            digest(&fs::read(directory.join(name))?)
        ));
    }
    fs::write(directory.join("checksums.txt"), checksums)?;
    let (server_name, server_digest) = published
        .server_image
        .split_once('@')
        .ok_or("server digest missing")?;
    let (browser_name, browser_digest) = published
        .browser_image
        .split_once('@')
        .ok_or("browser digest missing")?;
    output(github_output, "server_name", server_name)?;
    output(github_output, "browser_name", browser_name)?;
    output(github_output, "server_digest", server_digest)?;
    output(github_output, "browser_digest", browser_digest)?;
    output(
        github_output,
        "release_dir",
        directory.to_str().ok_or("release path is not UTF-8")?,
    )?;
    println!("published release manifest: {}", manifest.display());
    Ok(())
}

pub fn publish() -> Result<()> {
    let repository = env::var("GITHUB_REPOSITORY")?;
    let registry = valid_registry(&repository)?;
    let revision = env::var("GITHUB_SHA")?;
    let path = Path::new("reports/release")
        .join(&revision)
        .join("manifest.json");
    let local = release::load_and_verify(&path)?;
    validate_context(&env::var("GITHUB_REF")?, &revision, &local)?;
    let directory = path.parent().ok_or("release manifest lacks directory")?;
    login(&env::var("GITHUB_TOKEN")?, &env::var("GITHUB_ACTOR")?)?;
    let published = push_with(&RealRuntime, &local, directory, &registry)?;
    finalize(
        directory,
        &published,
        env::var("GITHUB_OUTPUT").ok().as_deref().map(Path::new),
    )
}

#[cfg(test)]
mod tests;

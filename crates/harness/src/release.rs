//! Local immutable-image build and evidence binding. Published signing is a separate gate.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub version: u32,
    pub source_commit: String,
    pub source_tree: String,
    pub server_image_id: String,
    pub browser_image_id: String,
    pub bundle_hash: String,
    pub protocol_version: u16,
    pub asset_pack_version: u32,
    pub rustc: String,
    pub e2e_report_hash: String,
    pub perf_report_hash: String,
}

fn output(program: &str, args: &[&str]) -> Result<String> {
    let result = Command::new(program).args(args).output()?;
    if !result.status.success() {
        return Err(format!(
            "{program} {args:?}: {}",
            String::from_utf8_lossy(&result.stderr)
        )
        .into());
    }
    Ok(String::from_utf8(result.stdout)?.trim().to_owned())
}

fn checked(program: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{program} {args:?} failed: {status}").into())
    }
}

fn collect_files(root: &Path, directory: &Path, paths: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            return Err(format!("bundle symlink: {}", path.display()).into());
        }
        if metadata.is_dir() {
            collect_files(root, &path, paths)?;
        } else if metadata.is_file() {
            paths.push(path.strip_prefix(root)?.to_path_buf());
        } else {
            return Err(format!("unsupported bundle entry: {}", path.display()).into());
        }
    }
    Ok(())
}

pub fn hash_bundle(root: &Path) -> Result<String> {
    let mut files = Vec::new();
    collect_files(root, root, &mut files)?;
    files.sort();
    if files.is_empty() {
        return Err("release bundle is empty".into());
    }
    let mut hasher = blake3::Hasher::new();
    for relative in files {
        let name = relative.to_str().ok_or("non-UTF-8 bundle filename")?;
        let bytes = fs::read(root.join(&relative))?;
        hasher.update(&(name.len() as u64).to_le_bytes());
        hasher.update(name.as_bytes());
        hasher.update(&(bytes.len() as u64).to_le_bytes());
        hasher.update(&bytes);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

fn evidence(path: &Path, revision: &str, result_key: &str) -> Result<String> {
    let bytes = fs::read(path)?;
    let value: Value = serde_json::from_slice(&bytes)?;
    if value["revision"] != revision || value[result_key] != "PASS" {
        return Err(format!("{} lacks PASS evidence for {revision}", path.display()).into());
    }
    if result_key == "verdict" && value["dirty"] != false {
        return Err("performance evidence was produced from a dirty tree".into());
    }
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

fn image_id(tag: &str) -> Result<String> {
    let id = output("docker", &["image", "inspect", "--format", "{{.Id}}", tag])?;
    if !id.starts_with("sha256:") {
        return Err(format!("invalid image ID for {tag}").into());
    }
    Ok(id)
}

pub fn build() -> Result<()> {
    let revision = output("git", &["rev-parse", "HEAD"])?;
    if !output("git", &["status", "--porcelain"])?.is_empty() {
        return Err("release build requires a clean source tree".into());
    }
    if output("git", &["branch", "--show-current"])? != "dev" {
        return Err("release build must start from dev".into());
    }
    let tree = output("git", &["rev-parse", "HEAD^{tree}"])?;
    let e2e = evidence(Path::new("reports/e2e/pass.json"), &revision, "result")?;
    let perf = evidence(Path::new("reports/perf/ci.json"), &revision, "verdict")?;
    let final_dir = PathBuf::from("reports/release").join(&revision);
    if final_dir.exists() {
        return Err(format!("release already exists: {}", final_dir.display()).into());
    }
    let staging =
        PathBuf::from("reports/release").join(format!("{revision}.staging-{}", std::process::id()));
    fs::create_dir_all(staging.join("bundle"))?;
    let artifact_tag = format!("aoeworld/artifacts:{revision}");
    let server_tag = format!("aoeworld/server:{revision}");
    let browser_tag = format!("aoeworld/browser:{revision}");
    let build_arg = format!("SOURCE_SHA={revision}");
    checked(
        "docker",
        &[
            "build",
            "-f",
            "docker/build-artifacts.Dockerfile",
            "--build-arg",
            &build_arg,
            "-t",
            &artifact_tag,
            ".",
        ],
    )?;
    let image_arg = format!("BUILD_IMAGE={artifact_tag}");
    checked(
        "docker",
        &[
            "build",
            "-f",
            "docker/server-runtime.Dockerfile",
            "--build-arg",
            &image_arg,
            "-t",
            &server_tag,
            ".",
        ],
    )?;
    checked(
        "docker",
        &[
            "build",
            "-f",
            "docker/browser-runtime.Dockerfile",
            "--build-arg",
            &image_arg,
            "-t",
            &browser_tag,
            ".",
        ],
    )?;
    let container = output("docker", &["create", &artifact_tag])?;
    let copy_result = checked(
        "docker",
        &[
            "cp",
            &format!("{container}:/source/web/."),
            staging
                .join("bundle")
                .to_str()
                .ok_or("non-UTF-8 staging path")?,
        ],
    );
    let remove_result = checked("docker", &["rm", "-f", &container]);
    copy_result?;
    remove_result?;
    let manifest = Manifest {
        version: 1,
        source_commit: revision,
        source_tree: tree,
        server_image_id: image_id(&server_tag)?,
        browser_image_id: image_id(&browser_tag)?,
        bundle_hash: hash_bundle(&staging.join("bundle"))?,
        protocol_version: aoe_protocol::VERSION,
        asset_pack_version: 1,
        rustc: output("rustc", &["--version"])?,
        e2e_report_hash: e2e,
        perf_report_hash: perf,
    };
    fs::copy("reports/e2e/pass.json", staging.join("e2e.json"))?;
    fs::copy("reports/perf/ci.json", staging.join("perf.json"))?;
    fs::write(
        staging.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    fs::rename(&staging, &final_dir)?;
    println!("{}", final_dir.join("manifest.json").display());
    Ok(())
}

pub fn load_and_verify(path: &Path) -> Result<Manifest> {
    let manifest: Manifest = serde_json::from_slice(&fs::read(path)?)?;
    if manifest.version != 1
        || manifest.protocol_version != aoe_protocol::VERSION
        || manifest.asset_pack_version != 1
    {
        return Err("release format or protocol mismatch".into());
    }
    if manifest.source_commit.len() != 40
        || !manifest
            .source_commit
            .bytes()
            .all(|b| b.is_ascii_hexdigit())
    {
        return Err("invalid source commit in release manifest".into());
    }
    let actual_tree = output(
        "git",
        &["rev-parse", &format!("{}^{{tree}}", manifest.source_commit)],
    )?;
    if actual_tree != manifest.source_tree {
        return Err("release source tree mismatch".into());
    }
    let directory = path.parent().ok_or("manifest lacks parent directory")?;
    if hash_bundle(&directory.join("bundle"))? != manifest.bundle_hash {
        return Err("release static bundle hash mismatch".into());
    }
    let server_tag = format!("aoeworld/server:{}", manifest.source_commit);
    let browser_tag = format!("aoeworld/browser:{}", manifest.source_commit);
    if image_id(&server_tag)? != manifest.server_image_id
        || image_id(&browser_tag)? != manifest.browser_image_id
    {
        return Err("release image ID mismatch".into());
    }
    if evidence(
        &directory.join("e2e.json"),
        &manifest.source_commit,
        "result",
    )? != manifest.e2e_report_hash
    {
        return Err("release E2E evidence mismatch".into());
    }
    if evidence(
        &directory.join("perf.json"),
        &manifest.source_commit,
        "verdict",
    )? != manifest.perf_report_hash
    {
        return Err("release performance evidence mismatch".into());
    }
    Ok(manifest)
}

pub fn verify(path: &Path) -> Result<()> {
    let manifest = load_and_verify(path)?;
    println!("verified local release {}", manifest.source_commit);
    Ok(())
}

pub fn rehearse(candidate: &Path, previous: &Path) -> Result<()> {
    let candidate = load_and_verify(candidate)?;
    let previous = load_and_verify(previous)?;
    crate::release_stack::rehearse(&candidate, &previous)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_hash_changes_on_tamper() {
        let directory = tempfile::tempdir().expect("tempdir");
        fs::write(directory.path().join("index.html"), "original").expect("write");
        let first = hash_bundle(directory.path()).expect("hash");
        fs::write(directory.path().join("index.html"), "tampered").expect("write");
        assert_ne!(first, hash_bundle(directory.path()).expect("hash"));
    }
}

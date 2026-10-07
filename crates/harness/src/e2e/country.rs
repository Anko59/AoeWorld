//! Explicit candidate-only source capture; never substitutes the fixed visual matrix.
use super::{BrowserContainer, Server, process, ready};
use std::{
    env, fs,
    net::TcpListener,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn validate_hash(hash: &str) -> Result<()> {
    if hash.len() != 64
        || !hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("country content hash must be 64 lowercase hexadecimal digits".into());
    }
    Ok(())
}

pub(super) fn run(directory: &Path, hash: &str) -> Result<()> {
    validate_hash(hash)?;
    let root = env::current_dir()?.canonicalize()?;
    let directory = directory.canonicalize()?;
    let pack = env::var_os("AOE_ASSET_PACK")
        .filter(|value| !value.is_empty())
        .ok_or("country browser qualification requires an original AOE_ASSET_PACK")?;
    let pack = Path::new(&pack).canonicalize()?;
    let assets = aoe_assets::pack::verify(&pack)?;
    let native = aoe_server::run_source_country_probe(&directory, hash)?;
    if !native.typed_hydrology || native.source_lock_count != 9 || native.start.is_none() {
        return Err("country browser qualification requires typed nine-source evidence and an ordinary start".into());
    }
    fs::create_dir_all(root.join("target"))?;
    let staged = tempfile::Builder::new()
        .prefix("country-source-")
        .tempdir_in(root.join("target"))?;
    super::landscape::stage_candidate(&directory, staged.path(), hash)?;
    let evidence_root = root.join("reports/country-source");
    fs::create_dir_all(&evidence_root)?;
    // Keep each run's artifacts; prior captures cannot satisfy a new attempt.
    let evidence = tempfile::Builder::new()
        .prefix("run-")
        .tempdir_in(&evidence_root)?
        .keep();
    fs::write(
        evidence.join("native.json"),
        serde_json::to_vec_pretty(&native)?,
    )?;
    fs::write(
        evidence.join("asset-manifest.json"),
        serde_json::to_vec_pretty(&assets)?,
    )?;
    let revision = Command::new("git").args(["rev-parse", "HEAD"]).output()?;
    if !revision.status.success() {
        return Err("could not record country capture revision".into());
    }
    let status = Command::new("git")
        .args(["status", "--porcelain"])
        .output()?;
    if !status.status.success() {
        return Err("could not record country source dirty status".into());
    }
    fs::write(
        evidence.join("inputs.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "policy": "explicit-candidate-country-browser-v1", "content_hash": hash,
            "revision": String::from_utf8(revision.stdout)?.trim(), "dirty": !status.stdout.is_empty(),
            "package_directory": directory, "asset_pack": pack,
            "limits": "temporary isolated server; original default unchanged; software GPU; not global traversal or hardware qualification"
        }))?,
    )?;
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    drop(listener);
    let mut server = Server(
        Command::new(root.join("target/release/aoe-server"))
            .current_dir(&root)
            .env("AOE_BIND", address.to_string())
            .env("AOE_SCENARIO", "smoke")
            .env_remove("AOE_MAP_WORKER")
            .env("AOE_MAP_PACKAGE_DIRECTORY", staged.path())
            .env("AOE_ASSET_PACK", &pack)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()?,
    );
    let deadline = Instant::now() + Duration::from_secs(60);
    while !ready(address) {
        if let Some(status) = server.0.try_wait()? {
            return Err(format!("country test server exited: {status}").into());
        }
        if Instant::now() >= deadline {
            return Err("country test server readiness exceeded60s".into());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let name = format!("aoeworld-country-{}-{}", std::process::id(), address.port());
    let _cleanup = BrowserContainer(name.clone());
    let user = format!(
        "{}:{}",
        nix::unistd::Uid::current(),
        nix::unistd::Gid::current()
    );
    let args = vec![
        "run".to_owned(),
        "--rm".into(),
        "--init".into(),
        "--network".into(),
        "host".into(),
        "--ipc".into(),
        "host".into(),
        "--user".into(),
        user,
        "--name".into(),
        name,
        "-e".into(),
        format!("HOME={}/.cache/browser-home", root.display()),
        "-e".into(),
        format!("AOE_BASE_URL=http://{address}"),
        "-e".into(),
        format!("AOE_COUNTRY_SOURCE_HASH={hash}"),
        "-e".into(),
        format!("AOE_COUNTRY_SOURCE_OUTPUT={}", evidence.display()),
        "-v".into(),
        format!("{}:{}", root.display(), root.display()),
        "-w".into(),
        root.join("browser").display().to_string(),
        "aoeworld/browser-tools:1.63.0".into(),
        "xvfb-run".into(),
        "-a".into(),
        "npx".into(),
        "playwright".into(),
        "test".into(),
        "tests/country/source.spec.ts".into(),
        "--project=webgpu".into(),
    ];
    let refs: Vec<_> = args.iter().map(String::as_str).collect();
    process::run("docker", &refs, Duration::from_secs(600))?;
    server.shutdown()?;
    let result: serde_json::Value =
        serde_json::from_slice(&fs::read(evidence.join("browser.json"))?)?;
    if result["observations"]
        .as_array()
        .is_none_or(|values| values.len() != 3)
        || result["content_hash"].as_str() != Some(hash)
    {
        return Err("country browser did not retain all three current-run captures".into());
    }
    println!("country source capture artifacts: {}", evidence.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn candidate_hash_validation_does_not_accept_paths_or_uppercase() {
        assert!(validate_hash(&"a".repeat(64)).is_ok());
        for value in [
            "",
            "../manifest",
            &"F".repeat(64),
            &"g".repeat(64),
            &"0".repeat(63),
        ] {
            assert!(validate_hash(value).is_err());
        }
    }
}

//! Ordinary source-backed creator journey across a full server restart.
use super::{BrowserContainer, Server, process, ready};
use aoe_map::MapPackage;
use std::{
    env, fs,
    net::{SocketAddr, TcpListener},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const BROWSER_DEADLINE: Duration = Duration::from_secs(1_800);

pub(super) fn run() -> Result<()> {
    let root = env::current_dir()?.canonicalize()?;
    let cache = PathBuf::from(
        env::var_os("AOE_GEODATA_CACHE").ok_or("test-creator-source requires AOE_GEODATA_CACHE")?,
    )
    .canonicalize()?;
    if !cache.is_dir() {
        return Err("AOE_GEODATA_CACHE must be a directory".into());
    }
    let worker = root.join("target/release/aoe-map-worker");
    if !worker.is_file() {
        return Err(format!("source worker is missing: {}", worker.display()).into());
    }
    let server_binary = root.join("target/release/aoe-server");
    if !server_binary.is_file() {
        return Err(format!("source test server is missing: {}", server_binary.display()).into());
    }
    let evidence_dir = root.join("reports/creator");
    fs::create_dir_all(&evidence_dir)?;
    for name in ["created.json", "reopened.json", "source.json"] {
        let path = evidence_dir.join(name);
        if path.exists() {
            fs::remove_file(path)?;
        }
    }
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    drop(listener);

    let mut server = start_server(&root, address, &cache, Some(&worker))?;
    run_browser(&root, address, "create", "webgpu")?;
    server.shutdown()?;

    let created = read_evidence(&root.join("reports/creator/created.json"))?;
    let hash = evidence_hash(&created)?;
    let manifest = root
        .join("local-assets/maps-v8")
        .join(format!("{hash}.json"));
    let package: MapPackage = serde_json::from_slice(&fs::read(&manifest)?)?;
    package.validate()?;
    if package.content_hash_hex() != hash || package.source_locks.is_empty() {
        return Err("creator did not publish a verified source-backed package".into());
    }
    process::run_with_env(
        worker.to_str().ok_or("worker path is not UTF-8")?,
        &["map-verify"],
        &[(
            "AOE_MAP_PACKAGE",
            manifest.to_str().ok_or("manifest path is not UTF-8")?,
        )],
        Duration::from_secs(120),
    )?;

    // The second server has no worker and an empty cache. Its successful
    // unseen-chunk request can therefore only use the published pages.
    let offline_cache = root.join(".cache/creator-offline");
    fs::create_dir_all(&offline_cache)?;
    let mut server = start_server(&root, address, &offline_cache, None)?;
    run_browser(&root, address, "reopen", "browser-defaults")?;
    server.shutdown()?;
    let reopened = read_evidence(&root.join("reports/creator/reopened.json"))?;
    if evidence_hash(&reopened)? != hash
        || reopened
            .get("offline_unseen_chunk")
            .and_then(serde_json::Value::as_bool)
            != Some(true)
    {
        return Err("source-backed reopen did not load the same offline package".into());
    }
    let revision = git(&["rev-parse", "HEAD"])?;
    let dirty = !git(&["status", "--porcelain"])?.is_empty();
    let report = serde_json::json!({
        "version": 1,
        "revision": revision,
        "dirty": dirty,
        "result": "PASS",
        "content_hash": hash,
        "generation_recipe_version": package.generation_recipe_version,
        "source_locks": package.source_locks,
        "projection": package.projection,
        "provenance": package.provenance,
        "environment": package.environment,
        "created": created,
        "reopened": reopened,
        "offline_policy": "server restarted without a map worker and with an empty source cache"
    });
    fs::write(
        root.join("reports/creator/source.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    Ok(())
}

fn start_server(
    root: &Path,
    address: SocketAddr,
    cache: &Path,
    worker: Option<&Path>,
) -> Result<Server> {
    let mut command = Command::new(root.join("target/release/aoe-server"));
    command
        .current_dir(root)
        .env("AOE_BIND", address.to_string())
        .env("AOE_SCENARIO", "smoke")
        .env("AOE_GEODATA_CACHE", cache)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    match worker {
        Some(worker) => {
            command.env("AOE_MAP_WORKER", worker);
        }
        None => {
            command.env_remove("AOE_MAP_WORKER");
        }
    }
    let mut server = Server(command.spawn()?);
    let deadline = Instant::now() + Duration::from_secs(60);
    while !ready(address) {
        if let Some(status) = server.0.try_wait()? {
            return Err(format!("creator server exited early: {status}").into());
        }
        if Instant::now() >= deadline {
            return Err("creator server did not become healthy within 60s".into());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Ok(server)
}

fn run_browser(root: &Path, address: SocketAddr, phase: &str, project: &str) -> Result<()> {
    let name = format!(
        "aoeworld-creator-{phase}-{}-{}",
        std::process::id(),
        address.port()
    );
    let _cleanup = BrowserContainer(name.clone());
    let user = format!(
        "{}:{}",
        nix::unistd::Uid::current(),
        nix::unistd::Gid::current()
    );
    let args = vec![
        "run".to_owned(),
        "--rm".to_owned(),
        "--init".to_owned(),
        "--network".to_owned(),
        "host".to_owned(),
        "--ipc".to_owned(),
        "host".to_owned(),
        "--user".to_owned(),
        user,
        "--name".to_owned(),
        name,
        "-e".to_owned(),
        format!("HOME={}/.cache/browser-home", root.display()),
        "-e".to_owned(),
        format!("AOE_BASE_URL=http://{address}"),
        "-e".to_owned(),
        format!("AOE_CREATOR_PHASE={phase}"),
        "-v".to_owned(),
        format!("{}:{}", root.display(), root.display()),
        "-w".to_owned(),
        root.join("browser").display().to_string(),
        "aoeworld/browser-tools:1.63.0".to_owned(),
        "xvfb-run".to_owned(),
        "-a".to_owned(),
        "npm".to_owned(),
        "run".to_owned(),
        "test:e2e".to_owned(),
        "--".to_owned(),
        "tests/source-creator.spec.ts".to_owned(),
        format!("--project={project}"),
    ];
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    process::run("docker", &refs, BROWSER_DEADLINE)?;
    Ok(())
}

fn read_evidence(path: &Path) -> Result<serde_json::Value> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn evidence_hash(value: &serde_json::Value) -> Result<&str> {
    let hash = value
        .get("content_hash")
        .and_then(serde_json::Value::as_str)
        .ok_or("creator evidence lacks content hash")?;
    if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("creator evidence has an invalid content hash".into());
    }
    Ok(hash)
}

fn git(args: &[&str]) -> Result<String> {
    let output = Command::new("git").args(args).output()?;
    if !output.status.success() {
        return Err("cannot read creator Git identity".into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use aoe_map::MapRequest;
    use serde::Deserialize;
    use std::collections::BTreeSet;

    #[derive(Deserialize)]
    struct Matrix {
        version: u16,
        year_ce: u16,
        cases: Vec<Case>,
    }

    #[derive(Deserialize)]
    struct Case {
        id: String,
        preparation: String,
        request: MapRequest,
    }

    #[test]
    fn fixed_geographic_matrix_has_valid_distinct_requests()
    -> Result<(), Box<dyn std::error::Error>> {
        let matrix: Matrix = serde_json::from_str(include_str!(
            "../../../../docs/geodata/reference-matrix.json"
        ))?;
        assert_eq!(matrix.version, 1);
        assert_eq!(matrix.year_ce, 600);
        assert_eq!(matrix.cases.len(), 11);
        let mut ids = BTreeSet::new();
        for case in matrix.cases {
            assert!(ids.insert(case.id));
            assert_eq!(case.preparation, "overview");
            assert_eq!(case.request.year_ce, matrix.year_ce);
            assert_eq!(case.request.normalized()?, case.request);
            case.request.estimate()?;
        }
        Ok(())
    }
}

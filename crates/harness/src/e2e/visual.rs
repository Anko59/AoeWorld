//! Source-backed fixed-region maps rendered through both browser backends.
use super::{BrowserContainer, Server, process, ready};
mod eviction;
mod packages;
use packages::{CaptureInputs, Result};
use std::{
    env, fs,
    net::{SocketAddr, TcpListener},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::Duration,
};

const BROWSER_DEADLINE: Duration = Duration::from_secs(1_200);
const MAX_SERVER_PEAK_RSS_BYTES: u64 = 1_073_741_824;

pub(super) fn run() -> Result<()> {
    let root = env::current_dir()?.canonicalize()?;
    let revision = git(&["rev-parse", "HEAD"])?;
    let (packages, inputs) = packages::prepare(&root, revision.clone())?;
    let evidence_dir = root.join("reports/geographic-visuals");
    fs::create_dir_all(&evidence_dir)?;
    let inputs_path = evidence_dir.join("cases.json");
    fs::write(&inputs_path, serde_json::to_vec_pretty(&inputs)?)?;

    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    drop(listener);
    let server_binary = root.join("target/release/aoe-server");
    if !server_binary.is_file() {
        return Err(format!(
            "visual capture server binary is missing: {}",
            server_binary.display()
        )
        .into());
    }
    let mut server_command = Command::new(server_binary);
    server_command
        .current_dir(&root)
        .env("AOE_BIND", address.to_string())
        .env("AOE_SCENARIO", "smoke")
        .env("AOE_MAP_PACKAGE_DIRECTORY", &packages.0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    let mut server = Server(server_command.spawn()?);
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    while !ready(address) {
        if let Some(status) = server.0.try_wait()? {
            return Err(format!("visual capture server exited before readiness: {status}").into());
        }
        if std::time::Instant::now() >= deadline {
            return Err("visual capture server did not become healthy within 60s".into());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let (stop_monitor, peak_memory, memory_monitor) = monitor_memory(server.0.id());
    let capture_result = run_browser(&root, address, &inputs_path);
    stop_monitor.store(true, Ordering::SeqCst);
    memory_monitor
        .join()
        .map_err(|_| "visual server memory monitor panicked")?;
    capture_result?;
    server.shutdown()?;
    verify_captures(&root, &inputs)?;
    let server_peak_memory_bytes =
        (peak_memory.load(Ordering::Relaxed) > 0).then(|| peak_memory.load(Ordering::Relaxed));
    let server_peak_memory_bytes =
        server_peak_memory_bytes.ok_or("visual capture did not collect a server VmRSS sample")?;
    if server_peak_memory_bytes > MAX_SERVER_PEAK_RSS_BYTES {
        return Err(format!(
            "visual capture server peak VmRSS exceeded the 1 GiB bound: {server_peak_memory_bytes} bytes"
        )
        .into());
    }
    let dirty = !git(&["status", "--porcelain"])?.is_empty();
    let activation_verdicts: serde_json::Value = serde_json::from_slice(&fs::read(
        root.join("reports/geographic-visuals/activation.json"),
    )?)?;
    let report = serde_json::json!({
        "version": 1,
        "result": "PASS",
        "capture_revision": revision,
        "prepared_revision": inputs.prepared_revision,
        "case_corrections": inputs.case_corrections,
        "dirty": dirty,
        "cases": inputs.cases,
        "activation_cases": inputs.activation_cases,
        "eviction_case": inputs.eviction_case,
        "activation_verdicts": activation_verdicts,
        "backends": ["webgpu", "canvas2d"],
        "page_work": {
            "matrix_elapsed_milliseconds": inputs.activation_cases.iter().map(|case| (case.id.as_str(), case.preparation_elapsed_milliseconds)).collect::<std::collections::BTreeMap<_, _>>(),
            "page_count": inputs.activation_cases.iter().map(|case| (case.id.as_str(), case.page_count)).collect::<std::collections::BTreeMap<_, _>>(),
            "page_bytes": inputs.activation_cases.iter().map(|case| (case.id.as_str(), case.page_bytes)).collect::<std::collections::BTreeMap<_, _>>(),
            "package_chunk_count_bound": inputs.activation_cases.iter().map(|case| (case.id.as_str(), case.package_chunk_count_bound)).collect::<std::collections::BTreeMap<_, _>>(),
            "server_peak_resident_bytes": server_peak_memory_bytes,
            "server_peak_resident_limit_bytes": MAX_SERVER_PEAK_RSS_BYTES
        },
        "limits": {
            "eviction": "source-backed Alpine package generated at 750x750 tiles; each backend exceeded 512 unique chunk requests, retained no more than 512 chunks, evicted and refetched the southeast relief target, and captured the restored view",
            "hardware": "browser rendering used Chromium SwiftShader; dedicated GPU qualification remains separate",
            "asset_pack": "recorded per capture; generated CI art fixtures are not equivalent to the original game pack",
            "memory": "server process VmRSS sampled during all browser work and bounded to 1 GiB; browser container memory and per-case server memory are not isolated"
        }
    });
    fs::write(
        evidence_dir.join("source.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    Ok(())
}

fn monitor_memory(pid: u32) -> (Arc<AtomicBool>, Arc<AtomicU64>, thread::JoinHandle<()>) {
    let stop = Arc::new(AtomicBool::new(false));
    let peak = Arc::new(AtomicU64::new(0));
    let stop_thread = Arc::clone(&stop);
    let peak_thread = Arc::clone(&peak);
    let task = thread::spawn(move || {
        let status = PathBuf::from(format!("/proc/{pid}/status"));
        while !stop_thread.load(Ordering::Relaxed) {
            if let Ok(contents) = fs::read_to_string(&status)
                && let Some(kilobytes) = contents.lines().find_map(|line| {
                    line.strip_prefix("VmRSS:")
                        .and_then(|value| value.split_whitespace().next())
                        .and_then(|value| value.parse::<u64>().ok())
                })
            {
                peak_thread.fetch_max(kilobytes.saturating_mul(1024), Ordering::Relaxed);
            }
            thread::sleep(Duration::from_millis(50));
        }
    });
    (stop, peak, task)
}

fn run_browser(root: &Path, address: SocketAddr, inputs: &Path) -> Result<()> {
    let name = format!("aoeworld-visual-{}-{}", std::process::id(), address.port());
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
        format!("AOE_SOURCE_VISUAL_CASES={}", inputs.display()),
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
        "tests/source-geographic-visuals.spec.ts".to_owned(),
        "--project=webgpu".to_owned(),
    ];
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    process::run("docker", &refs, BROWSER_DEADLINE)?;
    Ok(())
}

fn verify_captures(root: &Path, inputs: &CaptureInputs) -> Result<()> {
    let evidence = root.join("reports/geographic-visuals");
    let activation_path = evidence.join("activation.json");
    let activation: serde_json::Value = serde_json::from_slice(&fs::read(activation_path)?)?;
    let outcomes = activation
        .get("cases")
        .and_then(serde_json::Value::as_array)
        .ok_or("browser activation evidence has no cases array")?;
    if outcomes.len() != inputs.activation_cases.len() {
        return Err("browser activation evidence does not cover all fixed matrix cases".into());
    }
    let mut seen = std::collections::BTreeSet::new();
    for case in &inputs.activation_cases {
        let outcome = outcomes
            .iter()
            .find(|item| {
                item.get("case_id").and_then(serde_json::Value::as_str) == Some(case.id.as_str())
            })
            .ok_or_else(|| format!("browser activation evidence omits {}", case.id))?;
        if !seen.insert(case.id.as_str()) {
            return Err(format!("browser activation evidence duplicates {}", case.id).into());
        }
        if outcome
            .get("content_hash")
            .and_then(serde_json::Value::as_str)
            != Some(case.content_hash.as_str())
        {
            return Err(format!("browser activation hash mismatch for {}", case.id).into());
        }
        for (field, expected) in [
            (
                "preparation_elapsed_milliseconds",
                case.preparation_elapsed_milliseconds,
            ),
            ("page_count", u64::try_from(case.page_count)?),
            ("page_bytes", case.page_bytes),
            (
                "package_chunk_count_bound",
                u64::from(case.package_chunk_count_bound),
            ),
        ] {
            if outcome.get(field).and_then(serde_json::Value::as_u64) != Some(expected) {
                return Err(
                    format!("activation work metric {field} mismatches for {}", case.id).into(),
                );
            }
        }
        let active = outcome
            .get("start_available")
            .and_then(serde_json::Value::as_bool)
            .ok_or_else(|| format!("activation outcome for {} has no verdict", case.id))?;
        if active {
            let loaded_chunks = outcome
                .get("loaded_chunks")
                .and_then(serde_json::Value::as_array)
                .ok_or_else(|| format!("activation for {} has no loaded chunk list", case.id))?;
            if outcome
                .get("loaded_chunk_count")
                .and_then(serde_json::Value::as_u64)
                .is_none_or(|count| {
                    count == 0
                        || count > u64::from(case.package_chunk_count_bound)
                        || usize::try_from(count).ok() != Some(loaded_chunks.len())
                })
            {
                return Err(format!(
                    "activation for {} lacks bounded nonzero chunk work",
                    case.id
                )
                .into());
            }
            let image = evidence.join(&case.id).join("webgpu-overview.png");
            let metadata = evidence.join(&case.id).join("webgpu-overview.json");
            if !image.is_file() || fs::metadata(image)?.len() == 0 || !metadata.is_file() {
                return Err(
                    format!("activated map {} lacks a representative capture", case.id).into(),
                );
            }
        } else {
            if outcome.get("result").and_then(serde_json::Value::as_str)
                != Some("uninhabitable_preview_only")
            {
                return Err(
                    format!("preview-only verdict for {} is not classified", case.id).into(),
                );
            }
            if outcome
                .get("no_capture_reason")
                .and_then(serde_json::Value::as_str)
                .is_none_or(str::is_empty)
            {
                return Err(format!(
                    "uninhabitable map {} lacks an explicit no-capture reason",
                    case.id
                )
                .into());
            }
        }
    }
    for case in &inputs.cases {
        let active = outcomes
            .iter()
            .find(|item| {
                item.get("case_id").and_then(serde_json::Value::as_str) == Some(case.id.as_str())
            })
            .and_then(|item| item.get("start_available"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        if !active {
            continue;
        }
        for backend in ["webgpu", "canvas2d"] {
            let base = evidence.join(&case.id).join(backend);
            let image = base.with_extension("png");
            let metadata = base.with_extension("json");
            if !image.is_file() || fs::metadata(&image)?.len() == 0 || !metadata.is_file() {
                return Err(
                    format!("missing source-backed {backend} capture for {}", case.id).into(),
                );
            }
            for name in [
                format!("{backend}-initial.png"),
                format!("{backend}-traversed.png"),
                format!("{backend}-zoomed.png"),
            ] {
                let path = evidence.join(&case.id).join(name);
                if !path.is_file() || fs::metadata(path)?.len() == 0 {
                    return Err(format!(
                        "source-backed {backend} interaction capture for {} is missing",
                        case.id
                    )
                    .into());
                }
            }
            let record: serde_json::Value = serde_json::from_slice(&fs::read(metadata)?)?;
            let loaded_chunks = record
                .get("loaded_chunks")
                .and_then(serde_json::Value::as_array);
            if record
                .get("content_hash")
                .and_then(serde_json::Value::as_str)
                != Some(case.content_hash.as_str())
                || record.get("renderer").and_then(serde_json::Value::as_str) != Some(backend)
                || record
                    .get("prepared_revision")
                    .and_then(serde_json::Value::as_str)
                    != Some(inputs.prepared_revision.as_str())
                || record
                    .get("capture_revision")
                    .and_then(serde_json::Value::as_str)
                    != Some(inputs.capture_revision.as_str())
                || record
                    .get("source_locks")
                    .and_then(serde_json::Value::as_array)
                    .is_none_or(Vec::is_empty)
                || loaded_chunks.is_none_or(|chunks| {
                    chunks.is_empty()
                        || chunks.len()
                            > usize::try_from(case.package_chunk_count_bound).unwrap_or(usize::MAX)
                })
                || record
                    .pointer("/interactions/panned")
                    .and_then(serde_json::Value::as_bool)
                    != Some(true)
                || record
                    .pointer("/interactions/zoomed")
                    .and_then(serde_json::Value::as_bool)
                    != Some(true)
                || record
                    .pointer("/interactions/reconnected")
                    .and_then(serde_json::Value::as_bool)
                    != Some(true)
                || record
                    .pointer("/interactions/reloaded")
                    .and_then(serde_json::Value::as_bool)
                    != Some(true)
            {
                return Err(format!(
                    "source-backed {backend} identity, interaction or work evidence did not verify for {}",
                    case.id
                )
                .into());
            }
        }
    }
    eviction::verify_capture(&evidence, inputs)?;
    Ok(())
}

fn git(args: &[&str]) -> Result<String> {
    let output = Command::new("git").args(args).output()?;
    if !output.status.success() {
        return Err("cannot read visual qualification Git identity".into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

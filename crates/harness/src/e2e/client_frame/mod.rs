//! Fixed-work browser counters. Wall time is deliberately absent from the score.
use super::{BrowserContainer, Server, ready};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    net::TcpListener,
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
mod load;

#[cfg(test)]
mod tests;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(clap::Args, Debug)]
pub(crate) struct Options {
    #[arg(long)]
    package_directory: PathBuf,
    #[arg(long)]
    content_hash: String,
    #[arg(long)]
    asset_pack: PathBuf,
    #[arg(long, default_value = ".cache/client-frame-bench/report.json")]
    out: PathBuf,
    /// Add three runs while an owned, cold Dockerized Make compilation runs.
    #[arg(long)]
    compile_load: bool,
    /// Historical client artifact, served with this checkout's current map server.
    #[arg(long)]
    client_package_directory: Option<PathBuf>,
}

/// Six eight-frame holds: near, pan, far, far pan, near, return.
fn script(side: u64) -> Result<Vec<[f64; 3]>> {
    if side < 256 {
        return Err("benchmark map must be at least 256 tiles wide".into());
    }
    let c = (side / 2) as f64;
    Ok([
        [c, c, 1.0],
        [c + 32.0, c + 16.0, 1.0],
        [c + 32.0, c + 16.0, 0.25],
        [c - 32.0, c - 16.0, 0.25],
        [c - 32.0, c - 16.0, 1.0],
        [c, c, 1.0],
    ]
    .into_iter()
    .flat_map(|view| std::iter::repeat_n(view, 8))
    .collect())
}

#[derive(Serialize)]
struct Summary {
    means: Vec<f64>,
    spread_percent: Vec<f64>,
    max_frame_spread_percent: Vec<f64>,
}

fn summarize(runs: &[Vec<Vec<u64>>]) -> Result<Summary> {
    let frames = runs
        .first()
        .filter(|r| !r.is_empty())
        .ok_or("no frames")?
        .len();
    let width = runs[0][0].len();
    if width == 0
        || runs
            .iter()
            .any(|r| r.len() != frames || r.iter().any(|f| f.len() != width))
    {
        return Err("incomplete counter matrix".into());
    }
    let mut means = Vec::new();
    let mut spread_percent = Vec::new();
    let mut max_frame_spread_percent = Vec::new();
    for column in 0..width {
        let totals: Vec<f64> = runs
            .iter()
            .map(|run| {
                run.iter().map(|f| u128::from(f[column])).sum::<u128>() as f64 / frames as f64
            })
            .collect();
        let mean = totals.iter().sum::<f64>() / runs.len() as f64;
        let range = totals.iter().copied().fold(f64::NEG_INFINITY, f64::max)
            - totals.iter().copied().fold(f64::INFINITY, f64::min);
        means.push(mean);
        spread_percent.push(if mean == 0.0 {
            0.0
        } else {
            range / mean * 100.0
        });
        let mut worst = 0.0_f64;
        for frame in 0..frames {
            let values: Vec<f64> = runs.iter().map(|r| r[frame][column] as f64).collect();
            let average = values.iter().sum::<f64>() / values.len() as f64;
            let range = values.iter().copied().fold(f64::NEG_INFINITY, f64::max)
                - values.iter().copied().fold(f64::INFINITY, f64::min);
            if average > 0.0 {
                worst = worst.max(range / average * 100.0);
            }
        }
        max_frame_spread_percent.push(worst);
    }
    Ok(Summary {
        means,
        spread_percent,
        max_frame_spread_percent,
    })
}

#[derive(Deserialize, Serialize)]
struct Capture {
    counters: Vec<String>,
    frames: Vec<Vec<u64>>,
    wasm_sha256: String,
    bridge_sha256: String,
    chunks_sha256: String,
    video: Option<String>,
}

fn validate_captures(captures: &[Capture], frames: usize) -> Result<()> {
    let first = captures.first().ok_or("no captures")?;
    let labels = first.counters.len();
    let unique: std::collections::BTreeSet<_> = first.counters.iter().collect();
    if labels == 0 || unique.len() != labels || first.counters.iter().any(String::is_empty) {
        return Err("missing or duplicate counter labels".into());
    }
    if captures.iter().any(|c| {
        c.counters != first.counters
            || c.wasm_sha256 != first.wasm_sha256
            || c.bridge_sha256 != first.bridge_sha256
            || c.chunks_sha256 != first.chunks_sha256
            || c.frames.len() != frames
            || c.frames.iter().any(|f| f.len() != labels)
    }) {
        return Err("capture identity, labels or frame shape changed".into());
    }
    Ok(())
}

pub(crate) fn run(options: Options) -> Result<()> {
    let root = std::env::current_dir()?.canonicalize()?;
    let client_package_directory = options
        .client_package_directory
        .as_deref()
        .unwrap_or(std::path::Path::new(".cache/client-frame-bench/pkg"))
        .canonicalize()?;
    if !client_package_directory.starts_with(&root) {
        return Err("client package must be inside this checkout".into());
    }
    let package: aoe_map::MapPackage = serde_json::from_slice(&fs::read(
        options
            .package_directory
            .join(format!("{}.json", options.content_hash)),
    )?)?;
    package.validate()?;
    if package.content_hash_hex() != options.content_hash {
        return Err("package identity mismatch".into());
    }
    let script = script(package.estimate.tiles_per_side)?;
    fs::create_dir_all(options.out.parent().ok_or("output needs a directory")?)?;
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    drop(listener);
    let mut server = Server(
        Command::new(root.join("target/release/aoe-server"))
            .env("AOE_BIND", address.to_string())
            .env("AOE_SCENARIO", "smoke")
            .env("AOE_ASSET_PACK", options.asset_pack.canonicalize()?)
            .env(
                "AOE_MAP_PACKAGE_DIRECTORY",
                options.package_directory.canonicalize()?,
            )
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()?,
    );
    let deadline = Instant::now() + Duration::from_secs(60);
    while !ready(address) {
        if let Some(status) = server.0.try_wait()? {
            return Err(format!("benchmark server exited: {status}").into());
        }
        if Instant::now() >= deadline {
            return Err("benchmark server readiness timed out".into());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let config = options.out.with_extension("input.json");
    fs::write(
        &config,
        serde_json::to_vec(&serde_json::json!({
            "url": format!("http://{address}"), "content_hash": options.content_hash,
        "script": script, "side": package.estimate.tiles_per_side, "client_package_directory": client_package_directory,
        }))?,
    )?;
    let mut captures = Vec::<Capture>::new();
    let mut compile_load = None;
    for run in 0..if options.compile_load { 6 } else { 3 } {
        if run == 3 {
            compile_load = Some(load::CompileLoad::start(
                &options.out.with_extension("compile.log"),
            )?);
        }
        if let Some(load) = &mut compile_load {
            load.live()?;
        }
        let output = options.out.with_extension(format!("run-{run}.json"));
        let name = format!("aoeworld-client-frame-{}-{run}", std::process::id());
        let _cleanup = BrowserContainer(name.clone());
        let modules = root.join("browser/node_modules").canonicalize()?;
        let mut args = vec![
            "run".to_owned(),
            "--rm".into(),
            "--init".into(),
            "--network".into(),
            "host".into(),
            "--ipc".into(),
            "host".into(),
            "--name".into(),
            name,
            "--user".into(),
            format!(
                "{}:{}",
                nix::unistd::Uid::current(),
                nix::unistd::Gid::current()
            ),
            "-e".into(),
            format!("HOME={}/.cache/browser-home", root.display()),
            "-v".into(),
            format!("{}:{}", root.display(), root.display()),
            "-v".into(),
            format!("{}:{}:ro", modules.display(), modules.display()),
            "-w".into(),
            root.join("browser").display().to_string(),
            "aoeworld/browser-tools:1.64.0".into(),
            "node".into(),
            "bench/frame.mjs".into(),
            root.join(&config).display().to_string(),
            root.join(&output).display().to_string(),
            if run == 0 {
                "record".into()
            } else {
                "no-record".into()
            },
        ];
        args.splice(1..1, crate::jobs::cap_args());
        crate::process::run(
            "docker",
            &args.iter().map(String::as_str).collect::<Vec<_>>(),
            Duration::from_secs(900),
        )?;
        if let Some(load) = &mut compile_load {
            load.live()?;
        }
        captures.push(serde_json::from_slice(&fs::read(output)?)?);
    }
    drop(compile_load);
    server.shutdown()?;
    validate_captures(&captures, script.len())?;
    let summary = summarize(
        &captures
            .iter()
            .map(|c| c.frames.clone())
            .collect::<Vec<_>>(),
    )?;
    let repeatable = summary
        .spread_percent
        .iter()
        .chain(&summary.max_frame_spread_percent)
        .all(|spread| *spread <= 1.0);
    let git = |args: &[&str]| -> Result<String> {
        let output = Command::new("git").args(args).output()?;
        if !output.status.success() {
            return Err("git identity failed".into());
        }
        Ok(String::from_utf8(output.stdout)?.trim().to_owned())
    };
    let report = serde_json::json!({ "revision": git(&["rev-parse", "HEAD"])?, "dirty": !git(&["status", "--porcelain"])?.is_empty(),
        "client_revision": git(&["-C", client_package_directory.to_str().ok_or("non-UTF8 client path")?, "rev-parse", "HEAD"])?,
        "client_source_diff_blake3": blake3::hash(&Command::new("git").args(["-C", client_package_directory.to_str().ok_or("non-UTF8 client path")?, "diff", "HEAD", "--binary"]).output()?.stdout).to_hex().to_string(),
        "asset_manifest_blake3": blake3::hash(&fs::read(options.asset_pack.join("manifest.json"))?).to_hex().to_string(),
        "source_diff_blake3": blake3::hash(&Command::new("git").args(["diff", "HEAD", "--", "crates", "browser/bench", "make"]).output()?.stdout).to_hex().to_string(),
        "package": options.content_hash, "script": script, "viewport": [1280,720], "runs": captures,
        "summary": summary, "repeatable": repeatable, "idle_runs": 3, "compiling_runs": if options.compile_load {3} else {0},
        "idle": summarize(&captures[..3].iter().map(|c| c.frames.clone()).collect::<Vec<_>>())?,
        "compiling": if options.compile_load {Some(summarize(&captures[3..].iter().map(|c| c.frames.clone()).collect::<Vec<_>>())?)} else {None} });
    fs::write(&options.out, serde_json::to_vec_pretty(&report)?)?;
    for (index, name) in report["runs"][0]["counters"]
        .as_array()
        .ok_or("counter names missing")?
        .iter()
        .enumerate()
    {
        println!(
            "{name}: {} / frame; spread {}%",
            summary.means[index], summary.spread_percent[index]
        );
    }
    if !repeatable {
        return Err("counter spread exceeds 1%".into());
    }
    Ok(())
}

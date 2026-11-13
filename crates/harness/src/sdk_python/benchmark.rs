//! Reproducible long-run AI measurement; the Brain remains a public SDK client.
use super::*;

#[derive(Clone, Debug, Eq, PartialEq, clap::Args)]
pub(crate) struct BenchmarkArgs {
    #[arg(long, value_parser = package_hash)]
    map_hash: String,
    #[arg(long, default_value_t = 4, value_parser = clap::value_parser!(u32).range(1..=100))]
    bots: u32,
    #[arg(long, default_value_t = 1)]
    seed: u64,
    #[arg(long, default_value = ".cache/ai-scaling/starts.json")]
    starts: PathBuf,
    #[arg(long)]
    out: PathBuf,
    /// Wall-clock limit in minutes; the default is generous for the documented 120 game minutes.
    #[arg(long, default_value_t = DEFAULT_WALL_MINUTES, value_parser = clap::value_parser!(u64).range(1..))]
    wall_minutes: u64,
    /// Validate starting terrain without advancing the game or measuring performance.
    #[arg(long)]
    check_starts: bool,
}

/// The longest checkpoint is 120 game minutes; a measured run took about 2.4 wall minutes per game
/// minute, so 8 hours leaves a wide margin instead of truncating a valid run.
const DEFAULT_WALL_MINUTES: u64 = 8 * 60;

pub(super) fn wall_deadline(args: &BenchmarkArgs) -> Duration {
    Duration::from_secs(60 * args.wall_minutes)
}

fn package_hash(value: &str) -> Result<String, String> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("--map-hash needs 64 hexadecimal characters".into());
    }
    Ok(value.to_ascii_lowercase())
}

pub(super) fn run(root: &Path, args: &BenchmarkArgs) -> Result<(), Box<dyn Error>> {
    if !args.starts.is_file() {
        return Err(format!("start workload missing: {}", args.starts.display()).into());
    }
    process::run(
        "cargo",
        &["build", "--locked", "--release", "-p", "aoe-server"],
        STEP_DEADLINE,
    )?;
    let wheel = build_wheel(root, true, RELEASE_WHEELS)?;
    let site = root.join("target/sdk-python/benchmark-site");
    install(&wheel, &site)?;
    let python_path = format!("{}:{}", site.display(), root.join(AI_SOURCE).display());
    let captured = process::capture_in(
        root,
        "git",
        &["rev-parse", "HEAD"],
        &[],
        Duration::from_secs(10),
        &process::Cancellation::default(),
    );
    let revision = String::from_utf8(captured.stdout.clone())?;
    process::report_captured("git", captured, Duration::from_secs(10))?;
    let mut command = vec![
        "-m".to_owned(),
        "aoe_ai.benchmark".to_owned(),
        "--server-bin".into(),
        root.join("target/release/aoe-server").display().to_string(),
        "--map-hash".into(),
        args.map_hash.clone(),
        "--bots".into(),
        args.bots.to_string(),
        "--seed".into(),
        args.seed.to_string(),
        "--starts".into(),
        args.starts.display().to_string(),
        "--out".into(),
        args.out.display().to_string(),
    ];
    if args.check_starts {
        command.push("--check-starts".into());
    }
    let refs: Vec<_> = command.iter().map(String::as_str).collect();
    process::run_with_env(
        "python3",
        &refs,
        &[
            ("PYTHONPATH", &python_path),
            ("PYTHONDONTWRITEBYTECODE", "1"),
            ("AOE_BENCHMARK_REVISION", revision.trim()),
            (
                "AOE_ARENA_PACKAGES",
                &root.join("local-assets/map-packages").to_string_lossy(),
            ),
        ],
        wall_deadline(args),
    )
    .map_err(|error| {
        format!(
            "benchmark stopped early (wall limit {} minutes, raise --wall-minutes); the report at {} is incomplete (complete=false): {error}",
            args.wall_minutes,
            args.out.display()
        )
    })?;
    Ok(())
}

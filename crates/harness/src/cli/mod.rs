mod dispatch;

use crate::{agents, assets, e2e, gates, jobs, ledger, native, perf_scale, qa, review, ship};
use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "aoe-harness", about = "AoeWorld development harness")]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Assets {
        #[command(subcommand)]
        command: assets::AssetCommand,
    },
    Doctor,
    /// Prepare the bounded synthetic coast used by fishing captures.
    FishingScenario,
    DiskPrecheck,
    /// Classify every worktree; `--apply` removes merged clean ones and idle ones' target/.
    CleanStale {
        #[arg(long)]
        apply: bool,
    },
    /// Stop the harness containers that a host suspend left running.
    Recover,
    Dev {
        #[command(subcommand)]
        command: DevCommand,
    },
    StructureCheck,
    ArchitectureCheck,
    DocsCheck,
    /// Load documented local map packages and saves without running a match.
    CheckFixtures {
        /// Report a fixture that fails to load as a warning instead of failing.
        #[arg(long)]
        report: bool,
    },
    GatesDocs,
    ScopeCheck {
        #[arg(long)]
        revision: Option<String>,
    },
    GatePlan {
        #[arg(value_enum)]
        cadence: gates::registry::Cadence,
        #[arg(long)]
        base: Option<String>,
        paths: Vec<String>,
    },
    CoverageCheck {
        file: Option<PathBuf>,
    },
    Impact {
        #[arg(long)]
        base: Option<String>,
        paths: Vec<String>,
    },
    CiSelect,
    CiCheck,
    CiDispatchStatus,
    Fmt,
    FmtCheck,
    Lint,
    Deny,
    TestUnit,
    /// The slow source-qualification tests that `test-unit` excludes.
    TestSourceQualification,
    /// The seed sweeps and full-axis geodata round-trips that `test-unit` excludes.
    TestExhaustive,
    /// Print the nextest filter the coverage run uses (the test-unit tiers).
    CoverageFilter,
    LoadStatus,
    /// Lend one whole suite to agents and launched sessions for a while.
    Lease {
        target: String,
        #[arg(default_value_t = 60)]
        minutes: u64,
    },
    /// Report delivery stages over the last TIMELINE_DAYS (default seven).
    Timeline,
    /// Run a Make job, refusing immediately if another long job holds its lock.
    Job {
        gate: String,
        #[arg(last = true, required = true)]
        argv: Vec<String>,
    },
    TestWasm,
    TestWasmFocused,
    TestE2e,
    /// Regenerate duration-balanced E2E shard weights from Playwright JSON reports.
    E2eDurations(e2e::shard::DurationsArgs),
    /// Join the shards' evidence into `reports/e2e/pass.json`.
    E2eEvidence,
    /// Deterministic frame work counters on a current map package.
    ClientFrameBench(e2e::client_frame::Options),
    TestCreatorSource,
    TestGeographicMatrix,
    TestGeographicVisuals,
    FuzzSmoke,
    FuzzNightly,
    PerfSmoke,
    /// Three late-game profiles; optionally check replays and collect instructions in the same slot.
    PerfLateGame {
        #[arg(long)]
        check: bool,
        #[arg(long)]
        instructions: bool,
        /// Measure with economy sleep off (the replay oracle's other side).
        #[arg(long)]
        no_work_sleep: bool,
    },
    PerfCi,
    PerfFull,
    PerfPressure,
    PerfStress,
    /// Headless 100-player, 100,000-unit tick and server fan-out measurement.
    PerfScale(perf_scale::Options),
    /// Hundreds of armies crossing on open ground: the movement span split for group movement.
    PerfGroups(perf_scale::groups::Options),
    /// Callgrind instructions and last-level misses per movement stage, from `make perf-groups-instructions`.
    PerfStageCounters(perf_scale::stages::Options),
    /// The real server at scale: one human, Python AIs, a browser frame rate.
    ScaleMatch(perf_scale::scale_match::Options),
    /// Replay recorded owner-save commands without AIs, measuring instructions and thread CPU time.
    SaveReplay(perf_scale::save_replay::Options),
    /// Seeded gameplay quality metrics and revision-keyed history.
    QualityMetrics(crate::quality_metrics::Options),
    /// Closed-loop in-process late-game match with Rust bots: ticks/s, hashes per worker count, Callgrind stages.
    MatchBench(perf_scale::match_bench::Options),
    #[command(name = "perf-soak-10")]
    PerfSoak10,
    #[command(name = "perf-soak-30")]
    PerfSoak30,
    PerfBaselinePropose,
    /// Fail when the optimized gzip WASM is over its baseline cap (the perf-ci rule).
    PerfWasmSizeCheck,
    /// Post or rewrite the pull request's one informational CI performance comment.
    PerfComment,
    /// Write the latest passing dev instruction counts for the perf-ci gate (never fails the job).
    PerfDevReference,
    PerfHardwareCheck {
        #[arg(long, default_value = "reports/perf/hardware-environment.json")]
        environment: PathBuf,
        #[arg(long, default_value = "reports/perf/hardware-samples.json")]
        samples: PathBuf,
        #[arg(long, default_value = "baselines/perf/hardware.json")]
        baseline: PathBuf,
    },
    QaValidate(qa::Options),
    QaServe {
        #[arg(long, default_value = "fast")]
        budget: String,
    },
    GateRun(gates::runner::Options),
    ReviewSubject(gates::review::Options),
    PreCommit,
    /// The static checks, the unit tests, or (default) both; CI runs the two parts in parallel.
    Preflight {
        #[arg(long, value_enum, default_value = "all")]
        part: native::part::PreflightPart,
    },
    HooksInstall,
    HooksCheck,
    #[command(flatten)]
    Agents(agents::Commands),
    #[command(flatten)]
    Ship(ship::Commands),
    #[command(flatten)]
    Review(review::Commands),
    #[command(flatten)]
    Ledger(ledger::Commands),
    ReleaseBuild,
    ReleasePublish,
    ReleaseSourceCheck,
    ReleaseMainSourceCheck,
    ReleaseVerifyPublished,
    ReleaseRehearsePublished,
    ReleaseSmokePublished,
    ReleaseVerify {
        manifest: Option<PathBuf>,
    },
    ReleaseRehearse {
        candidate: Option<PathBuf>,
        previous: Option<PathBuf>,
    },
    RepoPolicyCheck,
    /// Build or test the Python bindings of the AI SDK (`make sdk-python`, `make sdk-python-test`).
    SdkPython {
        #[command(subcommand)]
        action: crate::sdk_python::Action,
    },
    SourceCountryProbe(e2e::CountryProbeArgs),
    SourceQualify {
        #[arg(long)]
        package_directory: PathBuf,
        #[arg(long)]
        content_hash: String,
        #[arg(long)]
        geographic_package_directory: Option<PathBuf>,
        #[arg(long)]
        geographic_content_hash: Option<String>,
        #[arg(long)]
        scale_512_package_directory: Option<PathBuf>,
        #[arg(long)]
        scale_512_content_hash: Option<String>,
        #[arg(long)]
        scale_16384_package_directory: Option<PathBuf>,
        #[arg(long)]
        scale_16384_content_hash: Option<String>,
        #[arg(long)]
        scale_262144_package_directory: Option<PathBuf>,
        #[arg(long)]
        scale_262144_content_hash: Option<String>,
        #[arg(long)]
        scale_only: bool,
        #[arg(long, default_value_t = 1_200_000)]
        max_ticks: u64,
    },
}
#[derive(Subcommand)]
enum DevCommand {
    Start,
    Down,
    Status,
    Logs,
}

pub(crate) fn main() {
    // The subcommand's name is the gate it runs: a heavy gate is the make target of that name.
    let matches = Args::command().get_matches();
    let gate = matches.subcommand_name().unwrap_or_default().to_owned();
    let args = Args::from_arg_matches(&matches).unwrap_or_else(|error| error.exit());
    // The one read of the long-job variables: every gate, hook and ship run below takes this.
    let slots = jobs::JobEnv::process();
    if let Err(error) = dispatch::run(args.command, &gate, &slots) {
        eprintln!("error: {error}");
        std::process::exit(jobs::lock::exit_code(error.as_ref()));
    }
}

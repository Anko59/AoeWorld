mod dispatch;

use crate::{
    agents, assets, e2e, gates, ledger, mutation, native, perf_scale, qa, review, ship, slots,
};
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
    /// Stop the harness containers that a host suspend left running.
    Recover,
    Dev {
        #[command(subcommand)]
        command: DevCommand,
    },
    StructureCheck,
    TestPlacement {
        #[arg(long)]
        index: bool,
        #[arg(long, conflicts_with = "index")]
        revision: Option<String>,
    },
    ArchitectureCheck,
    DocsCheck,
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
    LoadStatus,
    /// Report delivery stages over the last TIMELINE_DAYS (default seven).
    Timeline,
    /// Print the CPUs each heavy slot gives its containers (gates/heavy.json cores shared among its slots).
    SlotCpus,
    /// Run `argv` holding a heavy slot named `gate`: the make wrapper's steps always queue.
    Slot {
        gate: String,
        #[arg(last = true, required = true)]
        argv: Vec<String>,
    },
    TestWasm,
    TestWasmFocused,
    TestE2e,
    TestCreatorSource,
    TestGeographicMatrix,
    TestGeographicVisuals,
    FuzzSmoke,
    FuzzNightly,
    MutationNightly(mutation::Options),
    MutationIntegrity,
    PerfSmoke,
    PerfCi,
    PerfFull,
    PerfPressure,
    PerfStress,
    PerfTimingReport,
    /// Headless 100-player, 100,000-unit tick and server fan-out measurement.
    PerfScale(perf_scale::Options),
    /// The real server at scale: one human, Python AIs, a population cap, a browser frame rate.
    ScaleMatch(perf_scale::scale_match::Options),
    #[command(name = "perf-soak-10")]
    PerfSoak10,
    #[command(name = "perf-soak-30")]
    PerfSoak30,
    PerfBaselinePropose,
    /// Fail when the optimized gzip WASM is over its baseline cap (the perf-ci rule).
    PerfWasmSizeCheck,
    /// Post or rewrite the pull request's one informational CI performance comment.
    PerfComment,
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
    // The one read of the heavy-slot variables: every gate, hook and ship run below takes this.
    let slots = slots::SlotEnv::process();
    if let Err(error) = dispatch::run(args.command, &gate, &slots) {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

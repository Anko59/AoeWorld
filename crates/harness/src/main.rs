mod agents;
mod architecture;
mod assets;
mod coverage;
mod dev;
mod devserver;
mod e2e;
mod fuzz;
mod gates;
mod hooks;
mod input_json;
mod mutation;
mod native;
mod perf;
mod perf_comment;
mod perf_hardware;
mod perf_micro;
mod perf_pressure;
mod perf_size;
mod perf_soak;
mod perf_stress;
mod perf_timing;
mod policy;
mod process;
mod qa;
mod qa_mcp;
mod release;
mod release_promotion;
mod release_publish;
mod release_stack;
mod repo_policy;
mod review;
mod ship;
mod slots;
mod wasm_test;

use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

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
    ContractsCheck,
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
    TestUnit,
    /// The slow source-qualification tests that `test-unit` excludes.
    TestSourceQualification,
    LoadStatus,
    /// Run `argv` holding the heavy slot of gate `gate` (the registry decides whether it is heavy).
    Slot {
        gate: String,
        #[arg(last = true, required = true)]
        argv: Vec<String>,
    },
    TestWasm,
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

fn run(
    command: Command,
    gate: &str,
    slots: &slots::SlotEnv,
) -> Result<(), Box<dyn std::error::Error>> {
    // The agent hook takes no slot: a broken registry must not stop it judging edits.
    let _slot = match command {
        Command::Agents(_) => None,
        _ => Some(slots::guard::guard(gate, slots)?),
    };
    match command {
        Command::Assets { command } => assets::run(command)?,
        Command::Doctor => {
            for tool in ["git", "cargo", "rustc"] {
                process::run(tool, &["--version"], Duration::from_secs(10))?;
            }
        }
        Command::Dev { command } => match command {
            DevCommand::Start => dev::start()?,
            DevCommand::Down => dev::down()?,
            DevCommand::Status => dev::status()?,
            DevCommand::Logs => dev::logs()?,
        },
        Command::StructureCheck => policy::structure(Path::new("."))?,
        Command::TestPlacement { index, revision } => {
            let kind = revision.map_or_else(
                || {
                    if index {
                        gates::scopes::Kind::Index
                    } else {
                        gates::scopes::Kind::Working
                    }
                },
                gates::scopes::Kind::Commit,
            );
            gates::placement::check(Path::new("."), kind)?;
        }
        Command::ArchitectureCheck => architecture::check(Path::new("."))?,
        Command::ContractsCheck => println!("{}", gates::contracts::check(Path::new("."))?),
        Command::DocsCheck => gates::docs_check(Path::new("."))?,
        Command::GatesDocs => gates::docs_generate(Path::new("."))?,
        Command::ScopeCheck { revision } => {
            gates::scopes::inspect(Path::new("."), revision.as_deref())?
        }
        Command::GatePlan {
            cadence,
            base,
            paths,
        } => gates::plan(cadence, base.as_deref(), paths)?,
        Command::CoverageCheck { file } => {
            coverage::check(&file.unwrap_or_else(|| PathBuf::from("reports/coverage/native.lcov")))?
        }
        Command::Impact { base, paths } => gates::impact(base.as_deref(), paths)?,
        Command::CiSelect => gates::ci_select()?,
        Command::CiCheck => gates::ci_check()?,
        Command::CiDispatchStatus => gates::dispatch_status::run()?,
        Command::Fmt => format_commands(false)?,
        Command::FmtCheck => format_commands(true)?,
        Command::Lint => native::lint()?,
        Command::TestUnit => native::test_unit(native::affected::requested())?,
        Command::TestSourceQualification => native::test_source_qualification()?,
        Command::LoadStatus => slots::print_status(Path::new("."), slots)?,
        Command::Slot { gate, argv } => std::process::exit(slots::run_here(&gate, &argv, slots)?),
        Command::TestWasm => wasm_test::run()?,
        Command::TestE2e => e2e::run()?,
        Command::TestCreatorSource => e2e::run_source()?,
        Command::TestGeographicMatrix => e2e::run_matrix()?,
        Command::TestGeographicVisuals => e2e::run_visuals()?,
        Command::FuzzSmoke => fuzz::run(fuzz::Mode::Smoke)?,
        Command::FuzzNightly => fuzz::run(fuzz::Mode::Nightly)?,
        Command::MutationNightly(options) => mutation::run(options)?,
        Command::MutationIntegrity => mutation::run_integrity()?,
        Command::PerfSmoke => perf::run("smoke")?,
        Command::PerfCi => perf::run("ci")?,
        Command::PerfFull => perf::run("full")?,
        Command::PerfPressure => perf_pressure::run()?,
        Command::PerfStress => perf_stress::run()?,
        Command::PerfTimingReport => perf_timing::report()?,
        Command::PerfSoak10 => perf_soak::run(600, "soak-10")?,
        Command::PerfSoak30 => perf_soak::run(1_800, "soak-30")?,
        Command::PerfBaselinePropose => {
            perf_micro::propose()?;
            perf_size::propose()?;
        }
        Command::PerfWasmSizeCheck => perf_size::check()?,
        Command::PerfComment => perf_comment::run()?,
        Command::PerfHardwareCheck {
            environment,
            samples,
            baseline,
        } => perf_hardware::check(&environment, &samples, &baseline)?,
        Command::QaValidate(options) => qa::execute(options)?,
        Command::QaServe { budget } => qa_mcp::serve(&budget)?,
        Command::GateRun(options) => gates::runner::execute(Path::new("."), options, slots)?,
        Command::ReviewSubject(options) => gates::review::execute(Path::new("."), options)?,
        Command::PreCommit => {
            gates::scopes::static_checks(Path::new("."), gates::scopes::Kind::Index)?
        }
        Command::Preflight { part } => {
            if part.runs_checks() {
                gates::scopes::static_checks(Path::new("."), gates::scopes::Kind::Working)?;
            }
            if part.runs_unit() {
                let select = native::affected::preflight_base();
                {
                    let _slot = slots::guard::guard("test-unit", slots)?;
                    native::test_unit(select)?;
                }
                run(Command::PerfSmoke, "perf-smoke", slots)?;
            }
        }
        Command::HooksInstall => hooks::install(Path::new("."))?,
        Command::HooksCheck => hooks::check(Path::new("."))?,
        Command::Agents(command) => agents::execute(command, slots)?,
        Command::Ship(command) => ship::execute(command, slots)?,
        Command::Review(command) => review::execute(command, slots)?,
        Command::ReleaseBuild => release::build()?,
        Command::ReleasePublish => release_publish::publish()?,
        Command::ReleaseSourceCheck => release_promotion::source()?,
        Command::ReleaseMainSourceCheck => release_promotion::main_source()?,
        Command::ReleaseVerifyPublished => release_promotion::verify()?,
        Command::ReleaseRehearsePublished => release_promotion::rehearse()?,
        Command::ReleaseSmokePublished => release_promotion::smoke()?,
        Command::ReleaseVerify { manifest } => {
            let manifest = manifest
                .or_else(|| std::env::var_os("AOE_RELEASE_MANIFEST").map(PathBuf::from))
                .ok_or("release manifest required")?;
            release::verify(&manifest)?;
        }
        Command::ReleaseRehearse {
            candidate,
            previous,
        } => {
            let candidate = candidate
                .or_else(|| std::env::var_os("AOE_RELEASE_CANDIDATE").map(PathBuf::from))
                .ok_or("release candidate required")?;
            let previous = previous
                .or_else(|| std::env::var_os("AOE_RELEASE_PREVIOUS").map(PathBuf::from))
                .ok_or("previous release required")?;
            release::rehearse(&candidate, &previous)?;
        }
        Command::RepoPolicyCheck => repo_policy::check()?,
        Command::SourceCountryProbe(args) => e2e::country_probe(&args)?,
        Command::SourceQualify {
            package_directory,
            content_hash,
            geographic_package_directory,
            geographic_content_hash,
            scale_512_package_directory,
            scale_512_content_hash,
            scale_16384_package_directory,
            scale_16384_content_hash,
            scale_262144_package_directory,
            scale_262144_content_hash,
            scale_only,
            max_ticks,
        } => {
            let scale_packages = [
                (
                    512,
                    scale_512_package_directory,
                    scale_512_content_hash,
                ),
                (
                    16_384,
                    scale_16384_package_directory,
                    scale_16384_content_hash,
                ),
                (
                    262_144,
                    scale_262144_package_directory,
                    scale_262144_content_hash,
                ),
            ]
            .into_iter()
            .filter_map(|(tiles_per_side, directory, content_hash)| {
                match (directory, content_hash) {
                    (Some(directory), Some(content_hash)) => Some(Ok(
                        aoe_server::SourceScalePackageReference {
                            tiles_per_side,
                            directory,
                            content_hash,
                        },
                    )),
                    (None, None) => None,
                    _ => Some(Err(format!(
                        "scale {tiles_per_side} package directory and content hash must be supplied together"
                    ))),
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
            if scale_only {
                if geographic_package_directory.is_some() || geographic_content_hash.is_some() {
                    return Err(
                        "--scale-only cannot be combined with geographic route inputs".into(),
                    );
                }
                let mut references = scale_packages;
                references.push(aoe_server::SourceScalePackageReference {
                    tiles_per_side: 50_000,
                    directory: package_directory,
                    content_hash,
                });
                let evidence = aoe_server::run_source_scale_qualification(&references)?;
                println!("{}", serde_json::to_string_pretty(&evidence)?);
                return Ok(());
            }
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            let report = runtime.block_on(
                aoe_server::run_source_qualification_with_geographic_reference(
                    &package_directory,
                    &content_hash,
                    geographic_package_directory.as_deref(),
                    geographic_content_hash.as_deref(),
                    &scale_packages,
                    max_ticks,
                    |progress| {
                        eprintln!(
                            "source qualification: tick={} leg={} moved_m={:.1}",
                            progress.tick, progress.leg, progress.moved_meters
                        );
                    },
                ),
            )?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
    }
    Ok(())
}

fn format_commands(check: bool) -> Result<(), Box<dyn std::error::Error>> {
    for args in format_invocations(check) {
        process::run("cargo", &args, Duration::from_secs(120))?;
    }
    Ok(())
}

fn format_invocations(check: bool) -> [Vec<&'static str>; 2] {
    let mut root = vec!["fmt", "--all"];
    let mut fuzz = vec!["fmt", "--manifest-path", "fuzz/Cargo.toml"];
    if check {
        root.extend(["--", "--check"]);
        fuzz.extend(["--", "--check"]);
    }
    [root, fuzz]
}

fn main() {
    // The subcommand's name is the gate it runs: a heavy gate is the make target of that name.
    let matches = Args::command().get_matches();
    let gate = matches.subcommand_name().unwrap_or_default().to_owned();
    let args = Args::from_arg_matches(&matches).unwrap_or_else(|error| error.exit());
    // The one read of the heavy-slot variables: every gate, hook and ship run below takes this.
    let slots = slots::SlotEnv::process();
    if let Err(error) = run(args.command, &gate, &slots) {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
#[path = "gates/placement/main_tests.rs"]
mod main_tests;

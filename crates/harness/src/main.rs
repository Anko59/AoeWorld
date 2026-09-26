mod architecture;
mod coverage;
mod dev;
mod e2e;
mod fuzz;
mod gates;
mod mutation;
mod perf;
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
mod wasm_test;

use clap::{Parser, Subcommand};
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
        command: AssetCommand,
    },
    Doctor,
    Dev {
        #[command(subcommand)]
        command: DevCommand,
    },
    StructureCheck,
    ArchitectureCheck,
    DocsCheck,
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
    Fmt,
    FmtCheck,
    Lint,
    TestUnit,
    TestWasm,
    TestE2e,
    TestCreatorSource,
    TestGeographicMatrix,
    TestGeographicVisuals,
    FuzzSmoke,
    FuzzNightly,
    MutationNightly,
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
    PerfHardwareCheck {
        #[arg(long, default_value = "reports/perf/hardware-environment.json")]
        environment: PathBuf,
        #[arg(long, default_value = "reports/perf/hardware-samples.json")]
        samples: PathBuf,
        #[arg(long, default_value = "baselines/perf/hardware.json")]
        baseline: PathBuf,
    },
    QaValidate {
        file: Option<PathBuf>,
    },
    QaServe {
        #[arg(long, default_value = "fast")]
        budget: String,
    },
    PreCommit,
    Preflight,
    HooksInstall,
    HooksCheck,
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

#[derive(Subcommand)]
enum AssetCommand {
    Inspect { input: Option<PathBuf> },
    Import { input: Option<PathBuf> },
    Verify { pack: Option<PathBuf> },
}

fn run(command: Command) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        Command::Assets { command } => match command {
            AssetCommand::Inspect { input } => {
                let input = input.unwrap_or_else(|| PathBuf::from("local-assets/trial"));
                println!(
                    "{}",
                    serde_json::to_string_pretty(&aoe_assets::pack::inspect(&input)?)?
                );
            }
            AssetCommand::Import { input } => {
                let input = input.unwrap_or_else(|| {
                    let trial = PathBuf::from("local-assets/trial");
                    let game_data = trial.join("Data");
                    if game_data.is_dir() { game_data } else { trial }
                });
                println!(
                    "{}",
                    aoe_assets::pack::import(&input, Path::new("local-assets/packs"))?.display()
                );
            }
            AssetCommand::Verify { pack } => {
                let packs = if let Some(pack) = pack {
                    vec![pack]
                } else {
                    std::fs::read_dir("local-assets/packs")?
                        .filter_map(|item| item.ok().map(|value| value.path()))
                        .filter(|path| path.is_dir())
                        .collect()
                };
                if packs.is_empty() {
                    return Err("no local asset packs found".into());
                }
                for pack in packs {
                    let manifest = aoe_assets::pack::verify(&pack)?;
                    println!(
                        "verified {}: {} frames across {} pages",
                        pack.display(),
                        manifest.frames.len(),
                        manifest.pages.len()
                    );
                }
            }
        },
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
        Command::ArchitectureCheck => architecture::check(Path::new("."))?,
        Command::DocsCheck => gates::docs_check(Path::new("."))?,
        Command::CoverageCheck { file } => {
            coverage::check(&file.unwrap_or_else(|| PathBuf::from("reports/coverage/native.lcov")))?
        }
        Command::Impact { base, paths } => gates::impact(base.as_deref(), paths)?,
        Command::CiSelect => gates::ci_select()?,
        Command::CiCheck => gates::ci_check()?,
        Command::Fmt => format_commands(false)?,
        Command::FmtCheck => format_commands(true)?,
        Command::Lint => process::run(
            "cargo",
            &[
                "clippy",
                "--workspace",
                "--all-targets",
                "--locked",
                "--",
                "-D",
                "warnings",
            ],
            Duration::from_secs(600),
        )?,
        Command::TestUnit => {
            process::run(
                "cargo",
                &[
                    "nextest",
                    "run",
                    "--workspace",
                    "--locked",
                    "--no-fail-fast",
                ],
                Duration::from_secs(600),
            )?;
            process::run(
                "cargo",
                &["test", "--workspace", "--doc", "--locked"],
                Duration::from_secs(600),
            )?;
        }
        Command::TestWasm => wasm_test::run()?,
        Command::TestE2e => e2e::run()?,
        Command::TestCreatorSource => e2e::run_source()?,
        Command::TestGeographicMatrix => e2e::run_matrix()?,
        Command::TestGeographicVisuals => e2e::run_visuals()?,
        Command::FuzzSmoke => fuzz::run(fuzz::Mode::Smoke)?,
        Command::FuzzNightly => fuzz::run(fuzz::Mode::Nightly)?,
        Command::MutationNightly => mutation::run()?,
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
        Command::PerfHardwareCheck {
            environment,
            samples,
            baseline,
        } => perf_hardware::check(&environment, &samples, &baseline)?,
        Command::QaValidate { file } => {
            qa::validate_file(&file.unwrap_or_else(|| PathBuf::from("reports/qa/session.json")))?
        }
        Command::QaServe { budget } => qa_mcp::serve(&budget)?,
        Command::PreCommit => {
            run(Command::FmtCheck)?;
            run(Command::StructureCheck)?;
            run(Command::ArchitectureCheck)?;
            run(Command::DocsCheck)?;
            run(Command::Lint)?;
        }
        Command::Preflight => {
            run(Command::PreCommit)?;
            run(Command::TestUnit)?;
            run(Command::PerfSmoke)?;
        }
        Command::HooksInstall => {
            for (hook, command) in [("pre-commit", "pre-commit"), ("pre-push", "preflight")] {
                let path = gates::hook_path(hook)?;
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&path, format!("#!/bin/sh\nexec make {command}\n"))?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
                }
            }
        }
        Command::HooksCheck => {
            for (hook, expected) in [
                ("pre-commit", "exec make pre-commit"),
                ("pre-push", "exec make preflight"),
            ] {
                let path = gates::hook_path(hook)?;
                let content = std::fs::read_to_string(&path)?;
                if !content.contains(expected) {
                    return Err(format!("{hook} hook differs from expected dispatcher").into());
                }
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if std::fs::metadata(&path)?.permissions().mode() & 0o111 == 0 {
                        return Err(format!("{hook} is not executable").into());
                    }
                }
            }
        }
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
    if let Err(error) = run(Args::parse().command) {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_validation_includes_the_fuzz_workspace() {
        assert_eq!(
            format_invocations(false),
            [
                vec!["fmt", "--all"],
                vec!["fmt", "--manifest-path", "fuzz/Cargo.toml"],
            ]
        );
        assert_eq!(
            format_invocations(true),
            [
                vec!["fmt", "--all", "--", "--check"],
                vec!["fmt", "--manifest-path", "fuzz/Cargo.toml", "--", "--check"],
            ]
        );
    }
}

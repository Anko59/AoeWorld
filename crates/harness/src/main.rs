mod architecture;
mod coverage;
mod dev;
mod e2e;
mod gates;
mod perf;
mod perf_micro;
mod perf_size;
mod policy;
mod process;
mod qa;
mod qa_mcp;
mod release;
mod release_stack;
mod repo_policy;
mod wasm_test;

use clap::{Parser, Subcommand};
use std::{
    path::{Path, PathBuf},
    process::Command as OsCommand,
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
    Fmt,
    FmtCheck,
    Lint,
    TestUnit,
    TestWasm,
    TestE2e,
    PerfSmoke,
    PerfCi,
    PerfFull,
    PerfBaselinePropose,
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
    ReleaseVerify {
        manifest: Option<PathBuf>,
    },
    ReleaseRehearse {
        candidate: Option<PathBuf>,
        previous: Option<PathBuf>,
    },
    RepoPolicyCheck,
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
                let input = input.unwrap_or_else(|| PathBuf::from("local-assets/trial"));
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
        Command::Fmt => process::run("cargo", &["fmt", "--all"], Duration::from_secs(120))?,
        Command::FmtCheck => process::run(
            "cargo",
            &["fmt", "--all", "--", "--check"],
            Duration::from_secs(120),
        )?,
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
        Command::PerfSmoke => perf::run("smoke")?,
        Command::PerfCi => perf::run("ci")?,
        Command::PerfFull => perf::run("full")?,
        Command::PerfBaselinePropose => {
            perf_micro::propose()?;
            perf_size::propose()?;
        }
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
                let path = hook_path(hook)?;
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
                let path = hook_path(hook)?;
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
    }
    Ok(())
}

fn hook_path(name: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let result = OsCommand::new("git")
        .args(["rev-parse", "--git-path", &format!("hooks/{name}")])
        .output()?;
    if !result.status.success() {
        return Err("cannot locate Git hooks directory".into());
    }
    Ok(PathBuf::from(String::from_utf8(result.stdout)?.trim()))
}

fn main() {
    if let Err(error) = run(Args::parse().command) {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

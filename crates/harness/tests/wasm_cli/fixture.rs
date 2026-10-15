//! Rust process doubles: capture and retry orchestration remain production code.
use std::{fs, path::Path};

pub(super) fn wrapper(path: &Path, tool: &str) {
    use std::os::unix::fs::PermissionsExt;
    let executable = std::env::current_exe().unwrap();
    fs::write(path, format!("#!/bin/sh\nAOE_WASM_FIXTURE_TOOL='{tool}' AOE_WASM_FIXTURE_ARGS=\"$*\" exec '{}' --exact wasm_cli::fixture_process --nocapture\n", executable.display())).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

pub(super) fn run(tool: &str) {
    let root = std::path::PathBuf::from(std::env::var("AOE_WASM_FIXTURE_ROOT").unwrap());
    let args = std::env::var("AOE_WASM_FIXTURE_ARGS").unwrap();
    match tool {
        "cargo" => {
            let calls = root.join("calls");
            let count: usize = fs::read_to_string(&calls)
                .unwrap_or_else(|_| "0".into())
                .parse()
                .unwrap();
            fs::write(calls, (count + 1).to_string()).unwrap();
            if count == 0 || fs::read_to_string(root.join("repeat")).unwrap() == "true" {
                // Serial libtest prints its test label without a newline. Keep
                // the tool's first diagnostic on its own line on one-core CI.
                print!("\n{}", fs::read_to_string(root.join("diagnostic")).unwrap());
                let code = fs::read_to_string(root.join("exit"))
                    .unwrap()
                    .parse()
                    .unwrap();
                std::process::exit(code);
            }
        }
        "docker" if args.starts_with("run ") => {
            let port = args
                .split_whitespace()
                .find_map(|arg| arg.strip_prefix("--port="))
                .unwrap();
            fs::write(root.join("driver-port"), port).unwrap();
        }
        "docker" if args.starts_with("rm ") => {
            fs::write(root.join("driver-stopped"), "stopped").unwrap();
        }
        _ => panic!("unexpected fixture command: {tool} {args}"),
    }
    std::process::exit(0);
}

/// Exercise an owned slot after a test parent has admitted the workload. One
/// CPU makes ordinary host load exercise the regression without creating load.
pub(super) fn command(root: &Path) -> std::process::Command {
    let load: f64 = fs::read_to_string("/proc/loadavg")
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap();
    let mut command = if load > 1.0 {
        let status = fs::read_to_string("/proc/self/status").unwrap();
        let cpu = status
            .lines()
            .find_map(|line| line.strip_prefix("Cpus_allowed_list:"))
            .unwrap()
            .trim()
            .split([',', '-'])
            .next()
            .unwrap();
        eprintln!(
            "test-wasm load regression: host load {load}, child restricted to CPU {cpu} (1 core)"
        );
        let mut command = std::process::Command::new("taskset");
        command.args(["-c", cpu, env!("CARGO_BIN_EXE_aoe-harness")]);
        command
    } else {
        eprintln!("test-wasm overload rehearsal unavailable: load {load} <= 1");
        std::process::Command::new(env!("CARGO_BIN_EXE_aoe-harness"))
    };
    command
        .env_remove("AOE_HEAVY_SLOT")
        .env("AOE_HEAVY_DIR", root.join("slots"))
        .env("AOE_HEAVY_SLOTS", "1")
        .env("AOE_HEAVY_WAIT_MINUTES", "0")
        .env("AOE_HEAVY_LOAD_CHECKED", "1");
    command
}

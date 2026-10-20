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
            if let Ok(script) = fs::read(root.join("script.json")) {
                let script: Vec<Option<String>> = serde_json::from_slice(&script).unwrap();
                if let Some(Some(diagnostic)) = script.get(count) {
                    print!("\n{diagnostic}");
                    std::process::exit(1);
                }
                if let Some((_, name)) = args.split_once(" -- ") {
                    let name = name.strip_suffix(" --exact").expect("exact retry filter");
                    println!(
                        "\ntest {name} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 filtered out; finished in 0.01s"
                    );
                }
                std::process::exit(0);
            }
            let fail_until: usize = fs::read_to_string(root.join("fail-until"))
                .unwrap_or_else(|_| "1".into())
                .parse()
                .unwrap();
            if count < fail_until || fs::read_to_string(root.join("repeat")).unwrap() == "true" {
                // Serial libtest prints its test label without a newline. Keep
                // the tool's first diagnostic on its own line on one-core CI.
                print!("\n{}", fs::read_to_string(root.join("diagnostic")).unwrap());
                let code = fs::read_to_string(root.join("exit"))
                    .unwrap()
                    .parse()
                    .unwrap();
                std::process::exit(code);
            }
            if let Some((_, name)) = args.split_once(" -- ") {
                let name = name.strip_suffix(" --exact").expect("exact retry filter");
                println!(
                    "\ntest {name} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 filtered out; finished in 0.01s"
                );
            }
        }
        "docker" if args.starts_with("run ") => {
            let port = args
                .split_whitespace()
                .find_map(|arg| arg.strip_prefix("--port="))
                .unwrap();
            // Written whole, then renamed: the test polls this file and must
            // never read it half-written.
            fs::write(root.join("driver-port.tmp"), port).unwrap();
            fs::rename(root.join("driver-port.tmp"), root.join("driver-port")).unwrap();
        }
        "docker" if args.starts_with("rm ") => {
            fs::write(root.join("driver-stopped"), "stopped").unwrap();
        }
        _ => panic!("unexpected fixture command: {tool} {args}"),
    }
    std::process::exit(0);
}

/// Inherit a validated fixture slot after the test admits the workload. One
/// CPU makes ordinary host load exercise the regression without creating load.
pub(super) fn command() -> (std::process::Command, crate::ci::admission::FixtureSlot) {
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
        use std::io::Write;
        std::io::stdout().write_all(format!(
            "test-wasm load regression: host load {load}, child restricted to CPU {cpu} (1 core)\n"
        ).as_bytes()).unwrap();
        let mut command = std::process::Command::new("taskset");
        command.args(["-c", cpu, env!("CARGO_BIN_EXE_aoe-harness")]);
        command
    } else {
        eprintln!("test-wasm overload rehearsal unavailable: load {load} <= 1");
        std::process::Command::new(env!("CARGO_BIN_EXE_aoe-harness"))
    };
    let slot = crate::ci::admission::fixture(&mut command);
    (command, slot)
}

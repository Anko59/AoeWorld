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

/// Four ports of a window no other test shares, below the operating system's ephemeral range.
/// `test-wasm` picks its driver port and frees it, and the test then binds that same port to
/// answer the driver's readiness probe. With no range the harness took an ephemeral port, which
/// another process's `:0` bind or outgoing connection could take in between: `AddrInUse` once in
/// 23 full runs. The window comes from the process id and a counter, so tests in parallel processes
/// (nextest) and in parallel threads (`cargo test`) get different ones.
fn private_ports() -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let window = (std::process::id() * 16 + NEXT.fetch_add(1, Ordering::Relaxed)) % 4_000;
    let first = 12_000 + window * 4;
    format!("{first}-{}", first + 3)
}

/// Short WASM CLI fixtures start without queue tokens or load-dependent setup.
pub(super) fn command() -> std::process::Command {
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_aoe-harness"));
    command.env("AOE_E2E_PORT_RANGE", private_ports());
    command
}

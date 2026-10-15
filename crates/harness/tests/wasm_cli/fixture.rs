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
                print!("{}", fs::read_to_string(root.join("diagnostic")).unwrap());
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

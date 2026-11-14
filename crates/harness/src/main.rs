mod agents;
mod architecture;
mod assets;
mod browser_ports;
mod cli;
#[cfg(test)]
#[allow(dead_code)]
mod compile_cache;
mod coverage;
mod dev;
mod devserver;
mod disk;
mod e2e;
mod fixtures;
mod fuzz;
mod gates;
mod hooks;
mod input_json;
mod jobs;
mod ledger;
mod native;
mod perf;
mod perf_comment;
mod perf_hardware;
mod perf_micro;
mod perf_pressure;
mod perf_scale;
mod perf_size;
mod perf_soak;
mod perf_stress;
mod policy;
mod process;
mod qa;
mod qa_mcp;
mod quality_metrics;
mod release;
mod release_promotion;
mod release_publish;
mod release_stack;
mod repo_policy;
mod resources;
mod review;
mod sdk_python;
mod ship;
mod timeline;
mod wasm_test;
use std::time::Duration;

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
    cli::main();
}

#[cfg(test)]
#[path = "gates/main_tests.rs"]
mod main_tests;

#[cfg(test)]
#[path = "../examples/building-proof/tests/packing.rs"]
mod building_packing_tests;

#[cfg(test)]
#[path = "../examples/building-proof/tests/preparation.rs"]
mod building_preparation_tests;

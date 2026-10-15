//! Replay real diagnostics through the branch binary's test-wasm command.
use std::{
    fs,
    io::Read,
    net::TcpListener,
    path::Path,
    process::Stdio,
    time::{Duration, Instant},
};

mod fixture;

const CONTEXT_NAME: &str =
    "game_renderer::webgl_tests::webgl_three_native_materials_use_shared_barycentric_weights";
const PIXEL: &str = include_str!("../../src/wasm_test/tests/fixtures/pixel-38603174196.txt");
const CONTEXT: &str = include_str!("../../src/wasm_test/tests/fixtures/context-section.txt");

fn mixed() -> String {
    // Real, verbatim failure blocks, in a deliberately synthetic complete envelope.
    // The dev artifact cannot be replayed as a complete run: it was truncated at source.
    let start = PIXEL.find("---- ").unwrap();
    let end = PIXEL[start..].find("\nfailures:\n").unwrap() + start;
    let pixel_name = "game_renderer::webgl_tests::webgl_solid_surface_mode_does_not_sample_empty_atlas_or_degenerate_quad";
    format!(
        "failures:\n\n{CONTEXT}\n{}\nfailures:\n\n    {CONTEXT_NAME}\n    {pixel_name}\n\ntest result: FAILED. 0 passed; 2 failed; 0 ignored; 0 filtered out; finished in 1.00s\n",
        &PIXEL[start..end]
    )
}

#[test]
fn command_replays_real_failure_sections_and_only_retries_once() {
    let mut evidence = String::new();
    for (case, diagnostic, exit, repeat, calls, success) in [
        (
            "context plus pixel, second pass succeeds",
            mixed(),
            1,
            false,
            4,
            true,
        ),
        ("context plus pixel fails twice", mixed(), 1, true, 2, false),
        ("pixel-only artifact", PIXEL.into(), 1, false, 1, false),
        ("non-1 exit", mixed(), 2, false, 1, false),
        (
            "compile error",
            format!("error[E0308]: mismatched types\n{}", mixed()),
            1,
            false,
            1,
            false,
        ),
        ("empty", String::new(), 1, false, 1, false),
        (
            "truncated dev artifact",
            include_str!("../../src/wasm_test/tests/fixtures/dev-38560009588.txt").into(),
            1,
            false,
            1,
            false,
        ),
    ] {
        let cache = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.cache/webgl-fix/cli");
        fs::create_dir_all(&cache).unwrap();
        let root = tempfile::tempdir_in(cache).unwrap();
        let root = root.path();
        fs::write(root.join("diagnostic"), diagnostic).unwrap();
        fs::write(root.join("exit"), exit.to_string()).unwrap();
        fs::write(root.join("repeat"), repeat.to_string()).unwrap();
        let bin = root.join("bin");
        fs::create_dir(&bin).unwrap();
        for tool in ["cargo", "docker"] {
            fixture::wrapper(&bin.join(tool), tool);
        }
        let mut child = fixture::command(root)
            .current_dir(root)
            .env(
                "PATH",
                format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
            )
            .env("AOE_WASM_FIXTURE_ROOT", root)
            .args(["test-wasm"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let port = loop {
            if let Ok(port) = fs::read_to_string(root.join("driver-port")) {
                break port;
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                let mut stderr = String::new();
                child
                    .stderr
                    .take()
                    .unwrap()
                    .read_to_string(&mut stderr)
                    .unwrap();
                eprintln!("{stderr}");
            }
            assert!(Instant::now() < deadline, "driver startup request missing");
            std::thread::sleep(Duration::from_millis(10));
        };
        // Only startup readiness is doubled. This listener is owned by this test,
        // closes after wait_with_output, and never runs a browser or background child.
        let listener = TcpListener::bind(format!("127.0.0.1:{port}")).unwrap();
        let output = child.wait_with_output().unwrap();
        drop(listener);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let count: usize = fs::read_to_string(root.join("calls"))
            .unwrap()
            .parse()
            .unwrap();
        let summary = format!(
            "test-wasm CLI replay: {case}: exit={:?}, cargo calls={count}\n",
            output.status.code()
        );
        print!("{summary}");
        evidence.push_str(&summary);
        for line in stderr.lines().filter(|line| line.starts_with("test-wasm:")) {
            println!("{line}");
            evidence.push_str(line);
            evidence.push('\n');
        }
        fs::write(root.parent().unwrap().join("cli-replay.log"), &evidence).unwrap();
        assert_eq!(output.status.success(), success, "{case}: {stderr}");
        assert_eq!(count, calls, "{case}: {stderr}");
        assert_eq!(
            stderr.matches("retrying once").count(),
            usize::from(calls > 1),
            "{case}"
        );
        if calls > 1 {
            assert!(
                stderr.contains(CONTEXT_NAME),
                "must name the context failure"
            );
        }
        assert_eq!(root.join("reports/wasm/browser.json").exists(), success);
        assert!(
            root.join("driver-stopped").exists(),
            "driver must be cleaned up"
        );
    }
}

#[test]
fn fixture_process() {
    if let Ok(tool) = std::env::var("AOE_WASM_FIXTURE_TOOL") {
        fixture::run(&tool);
    }
}

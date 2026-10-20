use super::*;

const NAME: &str = "game_renderer::webgl_tests::tiling_pixels::webgl_blends_every_ground_material_pair_across_a_noisy_border";

#[test]
fn flaky_wasm_cli_keeps_one_quarantine_budget_across_the_webgl_fallback() {
    replay_budget(true);
}

#[test]
fn flaky_wasm_cli_preserves_the_remaining_retry_and_its_attempt_number() {
    replay_budget(false);
}

fn replay_budget(exhausted: bool) {
    let cache = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.cache/flaky-fixes/cli");
    fs::create_dir_all(&cache).unwrap();
    let directory = tempfile::tempdir_in(cache).unwrap();
    let root = directory.path();
    fs::create_dir(root.join("gates")).unwrap();
    let date = std::process::Command::new("date")
        .args(["-u", "+%Y-%m-%d"])
        .output()
        .unwrap();
    fs::write(root.join("gates/flaky.json"), serde_json::to_vec(&serde_json::json!([{
        "test":NAME, "gate":"test-wasm", "issue":"#391", "added":String::from_utf8(date.stdout).unwrap().trim(), "reason":"fallback budget"
    }])).unwrap()).unwrap();
    let pixel_name = "game_renderer::webgl_tests::webgl_solid_surface_mode_does_not_sample_empty_atlas_or_degenerate_quad";
    let pixel = PIXEL.replace(pixel_name, NAME);
    // Q recovers on retry 2; U forces a full rerun where Q fails again.
    // The old path retries Q twice more and incorrectly passes this command.
    let script = if exhausted {
        serde_json::json!([
            mixed().replace(pixel_name, NAME),
            pixel,
            null,
            PIXEL.replace(pixel_name, NAME),
            PIXEL.replace(pixel_name, NAME),
            null
        ])
    } else {
        serde_json::json!([mixed().replace(pixel_name, NAME), null, pixel, null])
    };
    fs::write(
        root.join("script.json"),
        serde_json::to_vec(&script).unwrap(),
    )
    .unwrap();
    let bin = root.join("bin");
    fs::create_dir(&bin).unwrap();
    for tool in ["cargo", "docker"] {
        fixture::wrapper(&bin.join(tool), tool);
    }
    let (mut command, _slot) = fixture::command();
    let mut child = command
        .current_dir(root)
        .env(
            "PATH",
            format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
        )
        .env("AOE_WASM_FIXTURE_ROOT", root)
        .arg("test-wasm")
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
            let output = child.wait_with_output().unwrap();
            panic!(
                "driver startup missing: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let listener = TcpListener::bind(format!("127.0.0.1:{port}")).unwrap();
    let output = child.wait_with_output().unwrap();
    drop(listener);
    let stderr = String::from_utf8_lossy(&output.stderr);
    fs::write(
        root.parent().unwrap().join(if exhausted {
            "budget-exhausted.log"
        } else {
            "budget-remaining.log"
        }),
        format!(
            "test-wasm fallback budget replay: exit={:?}, cargo calls={}\n{}\n{stderr}",
            output.status.code(),
            fs::read_to_string(root.join("calls")).unwrap(),
            String::from_utf8_lossy(&output.stdout)
        ),
    )
    .unwrap();
    println!(
        "test-wasm fallback budget replay: exit={:?}, cargo calls={}\n{stderr}",
        output.status.code(),
        fs::read_to_string(root.join("calls")).unwrap()
    );
    if exhausted {
        assert!(
            !output.status.success(),
            "spent quarantine budget must fail: {stderr}"
        );
        assert_eq!(fs::read_to_string(root.join("calls")).unwrap(), "4");
    } else {
        assert!(
            output.status.success(),
            "one remaining retry can recover: {stderr}"
        );
        assert_eq!(fs::read_to_string(root.join("calls")).unwrap(), "6");
    }
    assert_eq!(stderr.matches("retrying once").count(), 1);
    let records = fs::read_to_string(root.join("reports/wasm/quarantine.jsonl")).unwrap();
    let attempts: Vec<_> = records
        .lines()
        .map(|line| {
            serde_json::from_str::<serde_json::Value>(line).unwrap()["attempt"]
                .as_u64()
                .unwrap()
        })
        .collect();
    assert_eq!(
        attempts,
        if exhausted { vec![3] } else { vec![2, 3] },
        "{records}"
    );
    assert_eq!(root.join("reports/wasm/browser.json").exists(), !exhausted);
    assert!(root.join("driver-stopped").exists());
}

#[test]
fn flaky_wasm_cli_retries_only_quarantined_tests_and_records_recoveries() {
    for (listed, fail_until, expected_calls, success, attempt) in [
        (true, 1, 4, true, 2),
        (true, 2, 5, true, 3),
        (true, 3, 3, false, 0),
        (false, 1, 1, false, 0),
    ] {
        let cache = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.cache/flaky/cli");
        fs::create_dir_all(&cache).unwrap();
        let directory = tempfile::tempdir_in(cache).unwrap();
        let root = directory.path();
        fs::create_dir(root.join("gates")).unwrap();
        let date = std::process::Command::new("date")
            .args(["-u", "+%Y-%m-%d"])
            .output()
            .unwrap();
        let entries = if listed {
            serde_json::json!([{"test":NAME, "gate":"test-wasm", "issue":"#391",
            "added":String::from_utf8(date.stdout).unwrap().trim(), "reason":"CLI replay"}])
        } else {
            serde_json::json!([])
        };
        fs::write(
            root.join("gates/flaky.json"),
            serde_json::to_vec(&entries).unwrap(),
        )
        .unwrap();
        let original = "game_renderer::webgl_tests::webgl_solid_surface_mode_does_not_sample_empty_atlas_or_degenerate_quad";
        fs::write(root.join("diagnostic"), PIXEL.replace(original, NAME)).unwrap();
        fs::write(root.join("exit"), "1").unwrap();
        fs::write(root.join("repeat"), "false").unwrap();
        fs::write(root.join("fail-until"), fail_until.to_string()).unwrap();
        let bin = root.join("bin");
        fs::create_dir(&bin).unwrap();
        for tool in ["cargo", "docker"] {
            fixture::wrapper(&bin.join(tool), tool);
        }
        let (mut command, _slot) = fixture::command();
        let mut child = command
            .current_dir(root)
            .env(
                "PATH",
                format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
            )
            .env("AOE_WASM_FIXTURE_ROOT", root)
            .arg("test-wasm")
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
                let output = child.wait_with_output().unwrap();
                panic!(
                    "driver startup request missing: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let listener = TcpListener::bind(format!("127.0.0.1:{port}")).unwrap();
        let output = child.wait_with_output().unwrap();
        drop(listener);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.success(), success, "{stderr}");
        let calls: usize = fs::read_to_string(root.join("calls"))
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(calls, expected_calls, "{stderr}");
        assert!(
            !stderr.contains("retrying once"),
            "pixel failures never earn the context fallback"
        );
        let records = fs::read_to_string(root.join("reports/wasm/quarantine.jsonl")).unwrap();
        if success {
            assert!(
                stdout.contains(&format!(
                    "flaky: {NAME} passed on attempt {attempt} (quarantined, #391)"
                )),
                "{stdout}"
            );
            let record: serde_json::Value = serde_json::from_str(records.trim()).unwrap();
            assert_eq!(
                record,
                serde_json::json!({"test":NAME,"gate":"test-wasm","issue":"#391","attempt":attempt})
            );
        } else {
            assert!(records.is_empty());
        }
        assert_eq!(root.join("reports/wasm/browser.json").exists(), success);
        assert!(root.join("driver-stopped").exists());
    }
}

//! Live BROWSER_RUN regression: a child OOM must be reported despite parent exit 1.
use std::{os::unix::process::ExitStatusExt, process::Command};

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("--allocate") => {
            let mut allocations = Vec::new();
            loop {
                allocations.push(vec![1u8; 8 * 1024 * 1024]);
                std::hint::black_box(&allocations);
            }
        }
        Some("--child") => {
            let cap = std::fs::read_to_string("/sys/fs/cgroup/memory.max").unwrap();
            assert_eq!(
                cap.trim(),
                "134217728",
                "the probe must use its 128 MiB cgroup"
            );
            let events = std::fs::read_to_string("/sys/fs/cgroup/memory.events").unwrap();
            assert!(events.lines().any(|line| line == "oom_kill 0"), "{events}");
            let status = Command::new(std::env::current_exe().unwrap())
                .arg("--allocate")
                .status()
                .unwrap();
            eprintln!("OOM probe child signal={:?}", status.signal());
            // Like Playwright, the surviving parent turns a killed child into
            // a regular failure rather than exiting with Docker's OOM status.
            std::process::exit(if status.signal() == Some(9) { 1 } else { 3 });
        }
        None => {
            let result = Command::new("make")
                .args([
                    "--no-print-directory",
                    "resource-browser-oom-child",
                    "RESOURCE_CAP=--memory=128m --memory-swap=128m --cpu-shares=1024",
                ])
                .output()
                .unwrap();
            let stderr = String::from_utf8_lossy(&result.stderr);
            print!("{}", String::from_utf8_lossy(&result.stdout));
            eprint!("{stderr}");
            assert_eq!(result.status.code(), Some(2));
            assert!(
                stderr.contains("OOM probe child signal=Some(9)"),
                "{stderr}"
            );
            assert!(
                stderr.contains("Error 1"),
                "parent must exit normally: {stderr}"
            );
            assert!(
                stderr.contains("container exceeded its memory cap (kernel OOM kill)"),
                "{stderr}"
            );
        }
        _ => panic!("unexpected OOM probe argument"),
    }
}

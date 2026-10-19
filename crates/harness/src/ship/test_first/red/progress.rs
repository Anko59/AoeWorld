//! Live step supervision for the trusted red runner, independent of cold builds.
use crate::process::{Cancellation, Captured, capture_command_retaining_stdout};
use std::{
    io::{self, Write},
    process::Command,
    sync::mpsc::{self, Sender},
    thread,
    time::{Duration, Instant},
};

const PREFIX: &str = "RED_STEP ";

fn seconds(step: &str) -> Option<u64> {
    Some(match step {
        "runner-start" | "servers-up" | "playwright-discovery" => 60,
        "native-tests" | "browser-dependencies" | "server-build" | "wasm-build" | "wasm-tests" => {
            3600
        }
        "wasm-bindgen" => 120,
        "playwright-run" => 2400,
        "server-shutdown" | "runner-result" => 30,
        _ => return None,
    })
}

pub(crate) fn start(step: &str) {
    assert!(seconds(step).is_some(), "unknown red step: {step}");
    if std::env::var_os("AOE_RED_BROWSER_CONTAINER").is_some() {
        println!("{PREFIX}{step}");
    } else {
        println!("focused step {step}: start");
    }
    let _ = io::stdout().flush();
}

pub(super) fn capture(command: Command, cancellation: &Cancellation) -> Result<Captured, String> {
    capture_with_cancel(
        command,
        &|step| seconds(step).map(Duration::from_secs).unwrap_or_default(),
        cancellation,
    )
}

#[cfg(test)]
pub(super) fn capture_with(
    command: Command,
    budget: &(dyn Fn(&str) -> Duration + Sync),
) -> Result<Captured, String> {
    capture_with_cancel(command, budget, &Cancellation::default())
}

fn capture_with_cancel(
    command: Command,
    budget: &(dyn Fn(&str) -> Duration + Sync),
    cancellation: &Cancellation,
) -> Result<Captured, String> {
    let (sender, receiver) = mpsc::channel::<Option<String>>();
    println!(
        "red step runner-start: start (deadline {}s)",
        budget("runner-start").as_secs()
    );
    thread::scope(|scope| {
        let monitor = scope.spawn(move || {
            let mut step = "runner-start".to_owned();
            let mut started = Instant::now();
            let mut seen = std::collections::BTreeSet::from([step.clone()]);
            loop {
                let limit = budget(&step);
                if started.elapsed() >= limit {
                    cancellation.cancel();
                    return Some(format!(
                        "red step {step}: deadline after {:.2}s (budget {:.2}s)",
                        started.elapsed().as_secs_f64(),
                        limit.as_secs_f64()
                    ));
                }
                match receiver.recv_timeout(limit.saturating_sub(started.elapsed())) {
                    Ok(Some(next)) if seen.insert(next.clone()) => {
                        println!(
                            "red step {step}: finished in {:.2}s",
                            started.elapsed().as_secs_f64()
                        );
                        step = next;
                        started = Instant::now();
                        println!(
                            "red step {step}: start (deadline {}s)",
                            budget(&step).as_secs()
                        );
                    }
                    Ok(Some(_)) => (),
                    Ok(None) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                        println!(
                            "red step {step}: finished in {:.2}s",
                            started.elapsed().as_secs_f64()
                        );
                        return None;
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        cancellation.cancel();
                        return Some(format!(
                            "red step {step}: deadline after {:.2}s (budget {:.2}s)",
                            started.elapsed().as_secs_f64(),
                            limit.as_secs_f64()
                        ));
                    }
                }
            }
        });
        let mut output = Progress {
            sender: sender.clone(),
            line: Vec::new(),
            oversized: false,
            seen: std::collections::BTreeSet::new(),
        };
        // Each announced step has its own deadline. There is no competing total
        // that can cut off an otherwise in-budget cold build or browser suite.
        let captured =
            capture_command_retaining_stdout(command, Duration::MAX, cancellation, &mut output);
        let _ = sender.send(None);
        if let Some(error) = monitor.join().map_err(|_| "red step monitor panicked")? {
            return Err(format!(
                "{error}\n{}\n{}",
                String::from_utf8_lossy(&captured.stdout),
                String::from_utf8_lossy(&captured.stderr)
            ));
        }
        Ok(captured)
    })
}

struct Progress {
    sender: Sender<Option<String>>,
    line: Vec<u8>,
    oversized: bool,
    seen: std::collections::BTreeSet<String>,
}

impl Write for Progress {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        for &byte in bytes {
            if byte == b'\n' {
                if !self.oversized
                    && let Ok(line) = std::str::from_utf8(&self.line)
                    && let Some(step) = line.strip_prefix(PREFIX)
                    && seconds(step).is_some()
                    && self.seen.insert(step.to_owned())
                {
                    let _ = self.sender.send(Some(step.to_owned()));
                }
                self.line.clear();
                self.oversized = false;
            } else if self.line.len() < 4096 {
                self.line.push(byte);
            } else {
                self.oversized = true;
            }
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

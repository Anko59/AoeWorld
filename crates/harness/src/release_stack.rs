//! Disposable promotion and rollback of exact local image IDs.
use crate::release::Manifest;
use serde_json::Value;
use std::{
    error::Error,
    io::{Read, Write},
    net::TcpStream,
    process::Command,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

trait Runtime {
    fn docker(&self, args: &[&str]) -> Result<String>;
    fn healthy(&self, port: u16, revision: &str) -> bool;
}

struct RealRuntime;

impl Runtime for RealRuntime {
    fn docker(&self, args: &[&str]) -> Result<String> {
        let output = Command::new("docker").args(args).output()?;
        if !output.status.success() {
            return Err(format!(
                "docker {args:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            )
            .into());
        }
        Ok(String::from_utf8(output.stdout)?.trim().to_owned())
    }

    fn healthy(&self, port: u16, revision: &str) -> bool {
        health(port).is_ok_and(|value| value["status"] == "ok" && value["build"] == revision)
    }
}

struct Resources<'a, R: Runtime> {
    runtime: &'a R,
    network: String,
    containers: Vec<String>,
}

impl<R: Runtime> Drop for Resources<'_, R> {
    fn drop(&mut self) {
        for container in &self.containers {
            let _ = self.runtime.docker(&["rm", "-f", container]);
        }
        let _ = self.runtime.docker(&["network", "rm", &self.network]);
    }
}

struct Stack {
    server: String,
    browser: String,
}

struct Images<'a> {
    revision: &'a str,
    server: &'a str,
    browser: &'a str,
}

fn start<R: Runtime>(resources: &mut Resources<'_, R>, images: &Images<'_>) -> Result<Stack> {
    let server = resources.runtime.docker(&[
        "run",
        "-d",
        "--init",
        "--network",
        &resources.network,
        "--network-alias",
        "server",
        "--read-only",
        "--tmpfs",
        "/tmp:rw,noexec,nosuid,size=64m",
        images.server,
    ])?;
    resources.containers.push(server.clone());
    let browser = resources.runtime.docker(&[
        "run",
        "-d",
        "--init",
        "--network",
        &resources.network,
        "--read-only",
        "--tmpfs",
        "/tmp:rw,nosuid,size=64m",
        "--publish",
        "127.0.0.1::8080",
        images.browser,
    ])?;
    resources.containers.push(browser.clone());
    let stack = Stack { server, browser };
    wait_healthy(resources.runtime, &stack.browser, images.revision)?;
    Ok(stack)
}

fn stop<R: Runtime>(runtime: &R, stack: Stack) -> Result<()> {
    runtime.docker(&["rm", "-f", &stack.browser, &stack.server])?;
    Ok(())
}

fn health(port: u16) -> Result<Value> {
    let mut stream = TcpStream::connect_timeout(
        &format!("127.0.0.1:{port}").parse()?,
        Duration::from_secs(1),
    )?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")?;
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes)?;
    let response = String::from_utf8(bytes)?;
    if !response.starts_with("HTTP/1.1 200") {
        return Err(format!(
            "release stack health: {}",
            response.lines().next().unwrap_or("empty response")
        )
        .into());
    }
    let (_, body) = response
        .split_once("\r\n\r\n")
        .ok_or("health response lacks body")?;
    Ok(serde_json::from_str(body)?)
}

fn wait_healthy<R: Runtime>(runtime: &R, browser: &str, revision: &str) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(25);
    loop {
        let mapping = runtime.docker(&["port", browser, "8080/tcp"])?;
        if let Some(port) = mapping
            .lines()
            .next()
            .and_then(|line| line.rsplit(':').next())
            .and_then(|value| value.parse::<u16>().ok())
            && runtime.healthy(port, revision)
        {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!("release stack {revision} did not become healthy").into());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn rehearse_with<R: Runtime>(runtime: &R, candidate: &Manifest, previous: &Manifest) -> Result<()> {
    rehearse_images_with(
        runtime,
        &Images {
            revision: &candidate.source_commit,
            server: &candidate.server_image_id,
            browser: &candidate.browser_image_id,
        },
        &Images {
            revision: &previous.source_commit,
            server: &previous.server_image_id,
            browser: &previous.browser_image_id,
        },
    )
}

fn rehearse_images_with<R: Runtime>(
    runtime: &R,
    candidate: &Images<'_>,
    previous: &Images<'_>,
) -> Result<()> {
    if candidate.revision == previous.revision {
        return Err("promotion rehearsal requires two distinct source revisions".into());
    }
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let network = format!("aoeworld-release-{}-{nonce}", std::process::id());
    runtime.docker(&["network", "create", &network])?;
    let mut resources = Resources {
        runtime,
        network,
        containers: Vec::new(),
    };
    let previous_stack = start(&mut resources, previous)?;
    stop(runtime, previous_stack)?;
    let candidate_stack = start(&mut resources, candidate)?;
    stop(runtime, candidate_stack)?;
    let rollback_stack = start(&mut resources, previous)?;
    stop(runtime, rollback_stack)?;
    println!(
        "promoted {} and rolled back to {} using exact image references",
        candidate.revision, previous.revision
    );
    Ok(())
}

pub fn rehearse(candidate: &Manifest, previous: &Manifest) -> Result<()> {
    rehearse_with(&RealRuntime, candidate, previous)
}

pub fn rehearse_references(
    candidate_revision: &str,
    candidate_server: &str,
    candidate_browser: &str,
    previous_revision: &str,
    previous_server: &str,
    previous_browser: &str,
) -> Result<()> {
    rehearse_images_with(
        &RealRuntime,
        &Images {
            revision: candidate_revision,
            server: candidate_server,
            browser: candidate_browser,
        },
        &Images {
            revision: previous_revision,
            server: previous_server,
            browser: previous_browser,
        },
    )
}

fn smoke_images_with<R: Runtime>(runtime: &R, images: &Images<'_>) -> Result<()> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let network = format!("aoeworld-release-{}-{nonce}", std::process::id());
    runtime.docker(&["network", "create", &network])?;
    let mut resources = Resources {
        runtime,
        network,
        containers: Vec::new(),
    };
    stop(runtime, start(&mut resources, images)?)?;
    Ok(())
}

pub fn smoke_reference(revision: &str, server: &str, browser: &str) -> Result<()> {
    smoke_images_with(
        &RealRuntime,
        &Images {
            revision,
            server,
            browser,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    #[derive(Default)]
    struct FakeRuntime {
        calls: RefCell<Vec<Vec<String>>>,
        runs: Cell<usize>,
        fail_run: Option<usize>,
    }

    impl Runtime for FakeRuntime {
        fn docker(&self, args: &[&str]) -> Result<String> {
            self.calls
                .borrow_mut()
                .push(args.iter().map(|value| (*value).to_owned()).collect());
            match args {
                ["network", "create", ..] => Ok("network-id".to_owned()),
                ["run", ..] => {
                    let run = self.runs.get() + 1;
                    self.runs.set(run);
                    if self.fail_run == Some(run) {
                        return Err("injected Docker run failure".into());
                    }
                    Ok(format!("container-{run}"))
                }
                ["port", ..] => Ok("127.0.0.1:40000".to_owned()),
                ["rm", ..] | ["network", "rm", ..] => Ok(String::new()),
                _ => Err(format!("unexpected Docker call: {args:?}").into()),
            }
        }

        fn healthy(&self, port: u16, revision: &str) -> bool {
            port == 40000 && !revision.is_empty()
        }
    }

    fn manifest(revision: &str, server: &str, browser: &str) -> Manifest {
        Manifest {
            version: 1,
            source_commit: revision.to_owned(),
            source_tree: "tree".to_owned(),
            server_image_id: server.to_owned(),
            browser_image_id: browser.to_owned(),
            bundle_hash: "bundle".to_owned(),
            protocol_version: aoe_protocol::VERSION,
            asset_pack_version: 1,
            rustc: "test".to_owned(),
            e2e_report_hash: "e2e".to_owned(),
            perf_report_hash: "perf".to_owned(),
        }
    }

    #[test]
    fn promotion_uses_exact_images_and_rolls_back() {
        let runtime = FakeRuntime::default();
        let previous = manifest(
            "previous",
            "sha256:previous-server",
            "sha256:previous-browser",
        );
        let candidate = manifest(
            "candidate",
            "sha256:candidate-server",
            "sha256:candidate-browser",
        );
        assert!(rehearse_with(&runtime, &previous, &previous).is_err());
        assert!(runtime.calls.borrow().is_empty());
        rehearse_with(&runtime, &candidate, &previous).expect("rehearsal");
        assert_eq!(runtime.runs.get(), 6);
        let calls = runtime.calls.borrow();
        let images: Vec<_> = calls
            .iter()
            .filter(|args| args.first().is_some_and(|value| value == "run"))
            .filter_map(|args| args.last().cloned())
            .collect();
        assert_eq!(
            images,
            [
                "sha256:previous-server",
                "sha256:previous-browser",
                "sha256:candidate-server",
                "sha256:candidate-browser",
                "sha256:previous-server",
                "sha256:previous-browser",
            ]
        );
        assert!(
            calls
                .iter()
                .any(|args| args.first().is_some_and(|value| value == "network")
                    && args.get(1).is_some_and(|value| value == "rm"))
        );
    }

    #[test]
    fn failed_launch_cleans_only_its_own_resources() {
        let runtime = FakeRuntime {
            fail_run: Some(2),
            ..Default::default()
        };
        let previous = manifest("previous", "sha256:server", "sha256:browser");
        let candidate = manifest("candidate", "sha256:new-server", "sha256:new-browser");
        assert!(rehearse_with(&runtime, &candidate, &previous).is_err());
        let calls = runtime.calls.borrow();
        assert!(
            calls
                .iter()
                .any(|args| args == &["rm", "-f", "container-1"])
        );
        assert!(
            calls
                .iter()
                .any(|args| args.first().is_some_and(|value| value == "network")
                    && args.get(1).is_some_and(|value| value == "rm"))
        );
        assert!(
            !calls
                .iter()
                .any(|args| args.iter().any(|value| value == "container-2"))
        );
    }

    #[test]
    fn first_published_candidate_starts_without_a_previous_release() {
        let runtime = FakeRuntime::default();
        let images = Images {
            revision: "candidate",
            server: "ghcr.io/example/server@sha256:server",
            browser: "ghcr.io/example/browser@sha256:browser",
        };
        smoke_images_with(&runtime, &images).expect("candidate smoke");
        assert_eq!(runtime.runs.get(), 2);
        assert!(runtime.calls.borrow().iter().any(|args| {
            args.first().is_some_and(|name| name == "run")
                && args.last().is_some_and(|name| name == images.browser)
        }));
    }

    #[test]
    fn health_checks_actual_http_status_and_build_identity() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listener");
        let port = listener.local_addr().expect("port").port();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("request");
            let mut request = [0u8; 256];
            let _ = stream.read(&mut request);
            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 35\r\nConnection: close\r\n\r\n{\"status\":\"ok\",\"build\":\"candidate\"}").expect("response");
        });
        assert!(RealRuntime.healthy(port, "candidate"));
        server.join().expect("server");
    }
}

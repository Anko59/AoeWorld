use super::*;
use std::cell::{Cell, RefCell};

#[derive(Default)]
struct FakeRuntime {
    calls: RefCell<Vec<Vec<String>>>,
    exists: Cell<bool>,
    running: Cell<bool>,
    healthy: Cell<bool>,
    dies_on_start: bool,
}

impl Runtime for FakeRuntime {
    fn docker(&self, args: &[&str]) -> Result<String> {
        self.calls
            .borrow_mut()
            .push(args.iter().map(|value| (*value).to_owned()).collect());
        match args {
            ["ps", ..] => Ok(if self.exists.get() { "container" } else { "" }.to_owned()),
            ["inspect", ..] => Ok(self.running.get().to_string()),
            ["rm", ..] | ["stop", ..] => {
                self.exists.set(false);
                self.running.set(false);
                Ok(String::new())
            }
            ["run", ..] => {
                self.exists.set(true);
                self.running.set(!self.dies_on_start);
                Ok("container".to_owned())
            }
            _ => Err(format!("unexpected Docker call: {args:?}").into()),
        }
    }

    fn recent_logs(&self, _: &str, _: &str) -> Result<String> {
        Ok("startup failed".to_owned())
    }

    fn healthy(&self) -> bool {
        self.healthy.get()
    }
}

fn built_checkout() -> tempfile::TempDir {
    let temp = tempfile::tempdir().expect("checkout");
    let server = temp.path().join("target/release/aoe-server");
    let wasm = temp.path().join("web/pkg/aoe_client_bg.wasm");
    let worker = temp.path().join("target/release/aoe-map-worker");
    std::fs::create_dir_all(server.parent().expect("server parent")).expect("server dir");
    std::fs::create_dir_all(wasm.parent().expect("WASM parent")).expect("WASM dir");
    std::fs::write(server, b"server").expect("server");
    std::fs::write(wasm, b"wasm").expect("WASM");
    std::fs::write(&worker, b"worker").expect("worker");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&worker, std::fs::Permissions::from_mode(0o755))
            .expect("worker permissions");
    }
    temp
}

#[test]
fn checkout_names_are_stable_and_isolated() {
    assert_eq!(name(Path::new("/tmp/a")), name(Path::new("/tmp/a")));
    assert_ne!(name(Path::new("/tmp/a")), name(Path::new("/tmp/b")));
}

#[test]
fn development_build_identity_tracks_revision_and_dirty_state() {
    let checkout = tempfile::tempdir().expect("repository");
    assert!(build_identity(checkout.path()).is_err());
    git(checkout.path(), &["init", "-q"]).expect("init");
    std::fs::write(checkout.path().join("sample"), "initial").expect("file");
    git(checkout.path(), &["add", "sample"]).expect("add");
    git(
        checkout.path(),
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "-m",
            "seed",
        ],
    )
    .expect("commit");
    let revision = git(checkout.path(), &["rev-parse", "HEAD"]).expect("revision");
    assert_eq!(build_identity(checkout.path()).expect("clean"), revision);
    std::fs::write(checkout.path().join("sample"), "changed").expect("change");
    assert_eq!(
        build_identity(checkout.path()).expect("dirty"),
        format!("{revision}-dirty")
    );
}

#[test]
fn development_lifecycle_uses_checkout_name_and_cleans_stale_container() {
    let checkout = built_checkout();
    let runtime = FakeRuntime::default();
    runtime.exists.set(true);
    runtime.healthy.set(true);
    start_with(
        &runtime,
        checkout.path(),
        "smoke",
        "revision",
        Some("local-assets/packs/example"),
    )
    .expect("start");
    let calls = runtime.calls.borrow();
    assert!(
        calls
            .iter()
            .any(|args| args == &["rm", &name(checkout.path())])
    );
    let run = calls
        .iter()
        .find(|args| args.first().is_some_and(|arg| arg == "run"))
        .expect("Docker run");
    assert!(run.contains(&name(checkout.path())));
    assert!(run.contains(&"AOE_SCENARIO=smoke".to_owned()));
    assert!(run.contains(&"AOE_BUILD_SHA=revision".to_owned()));
    assert!(run.contains(&"AOE_ASSET_PACK=local-assets/packs/example".to_owned()));
    assert!(run.contains(&format!(
            "AOE_MAP_WORKER={}",
            checkout
                .path()
                .join("target/release/aoe-map-worker")
                .canonicalize()
                .expect("canonical worker")
                .display()
        )));
    assert!(run.contains(&format!(
            "AOE_GEODATA_CACHE={}",
            checkout
                .path()
                .join(aoe_server::Config::DEFAULT_GEODATA_CACHE_DIRECTORY)
                .canonicalize()
                .expect("canonical cache")
                .display()
        )));
    assert!(run.contains(&"--security-opt".to_owned()));
    assert!(run.contains(&"no-new-privileges".to_owned()));
    let mounts = run
        .windows(2)
        .filter_map(|pair| (pair[0] == "-v").then_some(pair[1].as_str()))
        .collect::<Vec<_>>();
    assert_eq!(mounts.len(), 3);
    let checkout_root = checkout.path().canonicalize().expect("checkout root");
    assert!(
        mounts.contains(
            &format!("{}:{}:ro", checkout_root.display(), checkout_root.display()).as_str()
        )
    );
    assert_eq!(
        mounts.iter().filter(|mount| mount.ends_with(":rw")).count(),
        2
    );
    let map_directory = checkout
        .path()
        .join(aoe_server::Config::DEFAULT_MAP_PACKAGE_DIRECTORY)
        .canonicalize()
        .expect("map directory");
    let cache_directory = checkout
        .path()
        .join(aoe_server::Config::DEFAULT_GEODATA_CACHE_DIRECTORY)
        .canonicalize()
        .expect("cache directory");
    assert!(
        mounts.contains(
            &format!("{}:{}:rw", map_directory.display(), map_directory.display()).as_str()
        )
    );
    assert!(
        mounts.contains(
            &format!(
                "{}:{}:rw",
                cache_directory.display(),
                cache_directory.display()
            )
            .as_str()
        )
    );
    assert!(run.contains(&"--read-only".to_owned()));
    drop(calls);
    status_with(&runtime, &name(checkout.path())).expect("running status");
    logs_with(&runtime, &name(checkout.path())).expect("logs");
    down_with(&runtime, &name(checkout.path())).expect("stop");
    assert!(!runtime.exists.get());
    assert!(logs_with(&runtime, &name(checkout.path())).is_err());
    down_with(&runtime, &name(checkout.path())).expect("idempotent stop");
    status_with(&runtime, &name(checkout.path())).expect("stopped status");
}

#[test]
fn invalid_configuration_or_missing_build_never_launches_docker_container() {
    let checkout = built_checkout();
    let runtime = FakeRuntime::default();
    assert!(start_with(&runtime, checkout.path(), "unknown", "revision", None).is_err());
    std::fs::remove_file(checkout.path().join("web/pkg/aoe_client_bg.wasm")).expect("remove WASM");
    assert!(start_with(&runtime, checkout.path(), "smoke", "revision", None).is_err());
    assert!(
        !runtime
            .calls
            .borrow()
            .iter()
            .any(|args| args.first().is_some_and(|arg| arg == "run"))
    );
}

#[test]
fn missing_or_nonexecutable_worker_never_launches_docker_container() {
    let checkout = built_checkout();
    let runtime = FakeRuntime::default();
    let worker = checkout.path().join("target/release/aoe-map-worker");
    assert!(is_executable(&worker));
    std::fs::remove_file(&worker).expect("remove worker");
    let missing = start_with(&runtime, checkout.path(), "smoke", "revision", None)
        .expect_err("missing worker");
    assert!(missing.to_string().contains("aoe-map-worker"));
    #[cfg(unix)]
    {
        std::fs::write(&worker, b"worker").expect("restore worker");
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&worker, std::fs::Permissions::from_mode(0o644))
            .expect("remove executable bit");
        let rejected = start_with(&runtime, checkout.path(), "smoke", "revision", None)
            .expect_err("nonexecutable worker");
        assert!(rejected.to_string().contains("aoe-map-worker"));
    }
    assert!(
        !runtime
            .calls
            .borrow()
            .iter()
            .any(|args| args.first().is_some_and(|arg| arg == "run"))
    );
}

#[cfg(unix)]
#[test]
fn writable_cache_mount_uses_canonical_target_for_a_symlink() {
    use std::os::unix::fs::symlink;
    let checkout = built_checkout();
    let external = tempfile::tempdir().expect("external cache");
    std::fs::create_dir_all(external.path().join("geodata")).expect("geodata");
    symlink(external.path(), checkout.path().join(".cache")).expect("cache symlink");
    let runtime = FakeRuntime::default();
    runtime.healthy.set(true);
    start_with(&runtime, checkout.path(), "smoke", "revision", None).expect("start");
    let calls = runtime.calls.borrow();
    let run = calls
        .iter()
        .find(|args| args.first().is_some_and(|arg| arg == "run"))
        .expect("Docker run");
    let cache = external
        .path()
        .join("geodata")
        .canonicalize()
        .expect("cache");
    assert!(run.contains(&format!("AOE_GEODATA_CACHE={}", cache.display())));
    assert!(run.contains(&format!("{}:{}:rw", cache.display(), cache.display())));
}

#[test]
fn failed_start_stops_only_its_checkout_container() {
    let checkout = built_checkout();
    let runtime = FakeRuntime {
        dies_on_start: true,
        ..Default::default()
    };
    let error = start_with(&runtime, checkout.path(), "smoke", "revision", None)
        .expect_err("failed start")
        .to_string();
    assert!(error.contains("startup failed"));
    let calls = runtime.calls.borrow();
    assert!(
        calls
            .iter()
            .any(|args| args == &["stop", &name(checkout.path())])
    );
}

#[test]
fn existing_running_lab_is_not_replaced() {
    let checkout = built_checkout();
    let runtime = FakeRuntime::default();
    runtime.exists.set(true);
    runtime.running.set(true);
    start_with(&runtime, checkout.path(), "smoke", "revision", None).expect("already running");
    assert!(
        !runtime
            .calls
            .borrow()
            .iter()
            .any(|args| matches!(args.first().map(String::as_str), Some("rm" | "run")))
    );
}

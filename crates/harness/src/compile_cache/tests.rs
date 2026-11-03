use super::*;

#[test]
fn compile_cache_nested_make_can_resolve_the_active_host_directory() {
    if env::var("SCCACHE_CLIENT_SIDE").as_deref() == Ok("1") {
        let directory = env::var("HARNESS_SCCACHE_DIR").unwrap();
        assert!(std::path::Path::new(&directory).is_dir());
    }
}

#[test]
fn compile_cache_disabled_adapter_preserves_arguments_environment_and_exit() {
    let result = Command::new("/usr/local/bin/aoe-compile-cache")
        .args([
            "/bin/sh",
            "-c",
            "printf '%s' \"$CARGO_MANIFEST_DIR\"; exit 17",
        ])
        .env_remove("AOE_COMPILE_ROOT")
        .env("CARGO_MANIFEST_DIR", "/original/source")
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(17));
    assert_eq!(result.stdout, b"/original/source");
}

#[test]
fn compile_cache_rewrites_only_complete_checkout_path_components() {
    assert_eq!(
        normalize("/tmp/tree/crates/core", "/tmp/tree"),
        "/aoe-source/crates/core"
    );
    assert_eq!(
        normalize("--out-dir=/tmp/tree/target", "/tmp/tree"),
        "--out-dir=/aoe-source/target"
    );
    assert_eq!(
        normalize("/tmp/tree-other/src", "/tmp/tree"),
        "/tmp/tree-other/src"
    );
    assert_eq!(normalize("/tmp/tree", "/tmp/tree"), "/aoe-source");
    assert_eq!(
        normalize("/other/tmp/tree/src", "/tmp/tree"),
        "/other/tmp/tree/src"
    );
}

#[test]
fn compile_cache_worktrees_produce_identical_compiler_inputs() {
    for root in ["/tmp/first", "/home/user/second"] {
        assert_eq!(
            normalize(&format!("{root}/target/debug/build/generated/out"), root),
            "/aoe-source/target/debug/build/generated/out"
        );
        assert_eq!(
            normalize(&format!("{root}/crates/core/src/lib.rs"), root),
            "/aoe-source/crates/core/src/lib.rs"
        );
    }
}

#[test]
fn compile_cache_is_wired_once_into_shared_mounts_and_environment() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let make = std::fs::read_to_string(root.join("Makefile")).unwrap();
    assert!(make.contains("RUSTC_WRAPPER=/usr/local/bin/aoe-compile-cache"));
    assert!(make.contains("$(call BIND_MOUNT,$(ROOT),/aoe-source)"));
    assert!(make.contains("$(call BIND_MOUNT,$(HARNESS_TARGET_CACHE),/aoe-source/target)"));
    assert!(make.contains("$(call BIND_MOUNT,$(HARNESS_SCCACHE_DIR),$(HARNESS_SCCACHE_DIR),1)"));
    assert!(make.contains("$(HOME)/.cache/aoe-sccache"));
    assert!(make.contains("SCCACHE_CLIENT_SIDE=1"));
    assert!(make.contains("$(wildcard $(HARNESS_SCCACHE_DIR)/server.sock)"));
    let jobs = make
        .lines()
        .find(|line| line.starts_with("JOB_ENV :="))
        .unwrap();
    assert!(jobs.contains("$(COMPILE_CACHE_ENV)"));
    assert!(jobs.contains("HARNESS_SCCACHE_DIR=$(HARNESS_SCCACHE_DIR)"));
    let mounts = make
        .lines()
        .find(|line| line.starts_with("ROOT_MOUNTS :="))
        .unwrap();
    assert!(mounts.contains("$(COMPILE_CACHE_MOUNTS)"));
}

#[test]
fn compile_cache_service_has_one_storage_owner_and_no_source_mount() {
    let args = service_arguments("/home/user/cache, with spaces", "1000:1000");
    assert!(
        args.iter()
            .any(|arg| arg == "SCCACHE_CACHE_SIZE=5368709120")
    );
    assert!(args.iter().any(|arg| arg == "SCCACHE_NO_DAEMON=1"));
    assert!(args.iter().any(|arg| arg == "SCCACHE_START_SERVER=1"));
    assert!(args.iter().any(|arg| arg == "--cpus=1"));
    assert!(args.iter().any(|arg| arg == "--memory=512m"));
    assert_eq!(
        args.iter().filter(|arg| arg.starts_with("--cpus")).count(),
        1
    );
    assert_eq!(
        args.iter()
            .filter(|arg| arg.starts_with("--memory"))
            .count(),
        1
    );
    assert!(args.iter().any(|arg| arg == "none"));
    assert!(!args.iter().any(|arg| arg.contains("/aoe-source")));
    assert_eq!(args.iter().filter(|arg| *arg == "--mount").count(), 1);
    assert_ne!(
        service_name("/cache/a", "1000:1000"),
        service_name("/cache/b", "1000:1000")
    );
}

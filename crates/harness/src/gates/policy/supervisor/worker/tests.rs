//! Fixture observations are explicitly not Docker execution or independent authority.
#![cfg(unix)]
use super::*;
use super::{
    config::Config,
    transport::{Binding, Cid},
};
use serde_json::{Value, json};
pub(super) fn config() -> Config {
    Config::parse(
        &serde_json::to_vec(
            &json!({"schema":1,"image":format!("sha256:{}","2".repeat(64)),
        "uid":65532,"memory_mib":128,"pids":32,"cpus":1,"workload_s":20,"cleanup_s":10,
        "command_s":1,"tmp_mib":16,"target_mib":64}),
        )
        .unwrap(),
    )
    .unwrap()
}
pub(super) fn binding(root: &Path) -> Binding {
    Binding::new(root, config(), Operation::FmtCheck, "3".repeat(32)).unwrap()
}
pub(super) fn cid() -> Cid {
    Cid::parse("a".repeat(64).as_bytes()).unwrap()
}
pub(super) fn image(b: &Binding) -> Value {
    json!({"Id":b.config.image,"Config":{"User":"65532:65532","WorkingDir":"/candidate",
        "Entrypoint":["/judge/aoe-harness"],"Cmd":null,"Env":transport::ENV}})
}
pub(super) fn inspect(b: &Binding, running: bool, exit: i32) -> Value {
    json!({"Id":cid().value(),"Image":b.config.image,
        "Config":{"Image":b.config.image,"User":"65532:65532","WorkingDir":"/candidate",
            "Entrypoint":["/judge/aoe-harness"],"Cmd":[b.operation.argument()],"Env":transport::ENV,
            "Labels":{"aoeworld.local-worker":"v1","aoeworld.local-nonce":b.nonce}},
        "HostConfig":{"NetworkMode":"none","ReadonlyRootfs":true,"Privileged":false,"CapDrop":["ALL"],
            "SecurityOpt":["no-new-privileges"],"Memory":134217728,"MemorySwap":134217728,"PidsLimit":32,"NanoCpus":1000000000,
            "Tmpfs":{"/tmp":"rw,nosuid,nodev,noexec,size=16m,mode=1777","/target":"rw,nosuid,nodev,exec,size=64m,mode=1777",
                "/scratch":"rw,nosuid,nodev,noexec,size=16m,mode=1777"},"AutoRemove":false,
            "RestartPolicy":{"Name":"no","MaximumRetryCount":0},
            "LogConfig":{"Type":"local","Config":{"max-size":"1m","max-file":"1","compress":"false"}},
            "PidMode":"","IpcMode":"private","UTSMode":"","CgroupnsMode":"private"},
        "Mounts":[{"Type":"bind","Source":b.source,"Destination":"/candidate","Mode":"","RW":false,"Propagation":"rprivate"}],
        "State":{"Dead":false,"Paused":false,"Restarting":false,"Running":running,"ExitCode":exit}})
}
#[test]
fn dispatch_is_only_six_operations_without_privileged_or_shell_escape() {
    let accepted = [
        "fmt-check",
        "structure-check",
        "architecture-check",
        "docs-check",
        "lint",
        "test-unit",
    ];
    for name in accepted {
        assert_eq!(operation(name).unwrap().argument(), name);
    }
    for name in [
        "publish",
        "approve",
        "sh -c true",
        "test-unit;true",
        "coverage",
        "fmt",
    ] {
        assert!(operation(name).is_none());
    }
    let observation = Observation::empty();
    assert!(!observation.authoritative);
    assert_eq!(observation.independent_judge, "UNAVAILABLE");
    assert_eq!(observation.status, Status::Unavailable);
    assert!(observation.transport.is_empty());
    assert_eq!(observation.container_exit_code, None);
    assert_eq!(observation.source_witness_unchanged, None);
}
mod canary;

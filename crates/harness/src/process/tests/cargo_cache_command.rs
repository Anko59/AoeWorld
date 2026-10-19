use super::command;
use std::process::Command;

#[test]
fn docker_fetch_preserves_option_values_that_look_like_network_flags() {
    let mut original = Command::new("docker");
    original.args([
        "run",
        "--rm",
        "--label",
        "--net=none",
        "--network",
        "none",
        "--",
        "tools:image",
        "cargo",
        "+stable",
        "build",
        "--offline",
        "--manifest-path=nested/Cargo.toml",
        "--config",
        "net.offline=true",
    ]);
    let fetched = command::fetch(&original).unwrap();
    let args: Vec<_> = fetched
        .get_args()
        .map(|arg| arg.to_str().unwrap())
        .collect();
    assert_eq!(
        args,
        [
            "run",
            "--rm",
            "--label",
            "--net=none",
            "-e",
            "CARGO_NET_OFFLINE=false",
            "tools:image",
            "cargo",
            "+stable",
            "fetch",
            "--locked",
            "--manifest-path=nested/Cargo.toml",
            "--config",
            "net.offline=true",
            "--config",
            "net.offline=false",
        ]
    );
}

#[test]
fn docker_fetch_does_not_guess_at_non_cargo_commands_or_unknown_options() {
    for args in [
        vec![
            "run",
            "--env",
            "cargo",
            "tools:image",
            "sh",
            "-c",
            "cargo build --offline",
        ],
        vec!["run", "--future-option", "tools:image", "cargo", "build"],
        vec!["run", "--env"],
        vec!["inspect", "cargo"],
    ] {
        let mut original = Command::new("docker");
        original.args(args);
        assert!(command::fetch(&original).is_none());
    }
}

use std::{path::Path, process::Command};

#[test]
fn focused_check_forwards_environment_before_each_docker_image() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut command = Command::new("make");
    command
        .args(["--no-print-directory", "--just-print", "duel-check-steps"])
        .current_dir(root);
    for name in [
        "MAKEFLAGS",
        "MFLAGS",
        "GNUMAKEFLAGS",
        "MAKEFILES",
        "MAKEOVERRIDES",
    ] {
        command.env_remove(name);
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = String::from_utf8(output.stdout).unwrap();
    let commands: Vec<_> = output
        .lines()
        .filter(|line| line.starts_with("docker run "))
        .collect();
    assert_eq!(commands.len(), 2);
    for (line, image) in commands
        .iter()
        .zip(["aoeworld/rust-tools:", "aoeworld/orchestrator:"])
    {
        let args: Vec<_> = line.split_whitespace().collect();
        let image_index = args.iter().position(|a| a.starts_with(image)).unwrap();
        for name in ["HARNESS_DUEL_BASE", "HARNESS_DUEL_REVISION"] {
            let env_index = args.iter().position(|a| *a == name).unwrap();
            assert!(env_index < image_index);
            assert_eq!(args[env_index - 1], "-e");
        }
        assert_eq!(args[image_index + 1], "cargo");
    }
}

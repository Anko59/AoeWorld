pub(super) mod admission;
mod selection;
mod status;

use std::{
    path::Path,
    process::{Command, Output},
};

fn command(root: &Path, name: &str) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_aoe-harness"));
    cmd.current_dir(root).arg(name);
    for key in [
        "AOE_BASE_SHA",
        "GITHUB_OUTPUT",
        "GITHUB_EVENT_NAME",
        "GITHUB_REF_NAME",
        "GITHUB_ACTIONS",
    ] {
        cmd.env_remove(key);
    }
    cmd
}

fn proof(cmd: &mut Command) -> Output {
    let output = cmd.output().unwrap();
    use std::io::Write;
    let text = format!(
        "$ {cmd:?}\n{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    std::io::stdout().write_all(text.as_bytes()).unwrap();
    output
}

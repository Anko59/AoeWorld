use std::{path::PathBuf, process::Command as OsCommand};

pub(crate) fn hook_path(name: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let result = OsCommand::new("git")
        .args(["rev-parse", "--git-path", &format!("hooks/{name}")])
        .output()?;
    if !result.status.success() {
        return Err("cannot locate Git hooks directory".into());
    }
    Ok(PathBuf::from(String::from_utf8(result.stdout)?.trim()))
}

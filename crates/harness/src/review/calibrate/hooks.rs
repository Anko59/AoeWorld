//! The role hooks of a case repository. A real review runs in a checkout whose
//! `.claude/`, `.codex/`, `.dsh/` and `.pi/` configuration sends every tool
//! call to the judge, which keeps a reviewer read-only. A case repository has no
//! harness sources to build a judge from, so it gets the same configuration
//! (read from the policy commit) and a launcher that runs the SOURCE
//! checkout's judge on the case repository.
use crate::review::trusted;
use std::{fs, path::Path};

/// The runtimes' hook configuration, copied verbatim.
pub(super) const CONFIG: [&str; 4] = [
    ".claude/settings.json",
    ".codex/hooks.json",
    ".dsh/hooks.json",
    ".pi/extensions/aoe-harness/index.ts",
];

/// Where every configuration looks for the launcher.
pub(super) const LAUNCHER: &str = ".agents/hooks/harness.sh";

/// The launcher: names the case repository as the checkout to judge, then runs
/// the source checkout's own launcher (and so its judge).
fn launcher(source: &Path) -> String {
    let quoted = source
        .join(LAUNCHER)
        .display()
        .to_string()
        .replace('\'', "'\\''");
    format!(
        "#!/bin/sh\n# review-calibrate: judge this case repository with the source checkout's judge.\n\
         AOE_AGENT_HOOK_ROOT=$(CDPATH= cd -- \"$(dirname -- \"$0\")/../..\" && pwd -P) || exit 2\n\
         export AOE_AGENT_HOOK_ROOT\nexec '{quoted}' \"$@\"\n"
    )
}

/// Write the hook configuration and launcher into `root`. A source without any
/// of them cannot enforce the reviewer role, so the case is refused.
pub(super) fn install(source: &Path, root: &Path) -> Result<(), String> {
    let source = source.canonicalize().map_err(|e| e.to_string())?;
    for relative in CONFIG {
        let text = trusted(&source, relative).map_err(|e| {
            format!("cannot run reviewers without their role hooks ({relative}): {e}")
        })?;
        super::repo::write(root, relative, &text)?;
    }
    super::repo::write(root, LAUNCHER, &launcher(&source))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let path = root.join(LAUNCHER);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

use super::*;
use std::time::Duration;

pub(super) fn run(source: &Path, kind: Kind) -> Result<()> {
    let source = fs::canonicalize(source)?;
    let snapshot = Snapshot::prepare(&source, kind)?;
    let target = source.join("target");
    let target = target.to_str().ok_or("non-UTF-8 Cargo target path")?;
    // Cache/outputs are outside the exported input; ignored evidence is not copied.
    let cargo = |args: &[&str], budget: u64| -> Result<()> {
        snapshot.run_checked(|root| {
            crate::process::run_in(
                root,
                "cargo",
                args,
                &[("CARGO_TARGET_DIR", target)],
                Duration::from_secs(budget),
            )?;
            Ok(())
        })
    };
    for args in crate::format_invocations(true) {
        cargo(&args, 120)?;
    }
    snapshot.run_checked(|root| crate::policy::structure(root).map_err(|error| error.into()))?;
    snapshot.run_checked(crate::architecture::check)?;
    snapshot.run_checked(crate::gates::docs_check)?;
    cargo(
        &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--locked",
            "--",
            "-D",
            "warnings",
        ],
        600,
    )?;
    println!(
        "static input: {}",
        serde_json::to_string(&snapshot.identity)?
    );
    Ok(())
}

use super::*;
use crate::{
    gates::{
        runner::{evidence::PrivateOutput, signals::Signals},
        scopes::{Kind, Snapshot},
    },
    process::{self, Cancellation, CaptureExit},
};
use std::time::Duration;
#[derive(clap::Args)]
pub(crate) struct Options {
    /// Existing external launcher configuration, never candidate origin.
    #[arg(long)]
    anchor: PathBuf,
    #[arg(long)]
    candidate: String,
    #[arg(long, value_enum, default_value = "pr")]
    cadence: Cadence,
    /// Existing external owner-writable evidence directory.
    #[arg(long)]
    output: PathBuf,
}
pub(crate) fn execute(root: &Path, options: Options) -> Result<()> {
    let source = fs::canonicalize(root)?;
    let git_common = crate::hooks::common_directory(&source)?;
    let output = PrivateOutput::new(&options.output, &[source.clone(), git_common])?;
    output.reject_input_alias(&options.anchor, "anchor")?;
    // Reused directories cannot retain a stale ready descriptor after ANY error.
    let pending = serde_json::json!({"schema":1,"authoritative":false,"status":"UNAVAILABLE","reasons":["preparation in progress; not execution evidence"]});
    output.atomic("preparation.json", &serde_json::to_vec_pretty(&pending)?)?;
    let cancellation = Cancellation::default();
    let signals = Signals::new(&cancellation);
    let result = (|| -> Result<Preparation> {
        signals.as_ref().map_err(|error| error.to_string())?;
        full_oid(&options.candidate)?;
        let anchor_path = plain_absolute(&options.anchor)?;
        if anchor_path.starts_with(&source)
            || !fs::symlink_metadata(&anchor_path)?.file_type().is_file()
        {
            return Err("anchor must be an external regular file".into());
        }
        let anchor_path = regular(
            anchor_path.parent().ok_or("anchor parent missing")?,
            anchor_path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or("anchor filename invalid")?,
        )?;
        let anchor_bytes = fs::read(&anchor_path)?;
        let anchor = Anchor::parse(&anchor_bytes)?;
        let candidate = Snapshot::prepare(&source, Kind::Commit(options.candidate.clone()))?;
        let resolved = backend::resolve(&anchor, &output, &cancellation)?;
        let _owner_path = resolved.owner.path();
        let policy = Materialized::from_snapshot(&resolved.snapshot, resolved.identity.clone())?;
        let tree = candidate
            .identity
            .tree
            .as_deref()
            .ok_or("candidate tree missing")?;
        let paths = vec![".unknown-policy-comparison".into()];
        let mut prepared = Preparation::prepare(
            &policy,
            &options.candidate,
            tree,
            &paths,
            options.cadence,
            &BTreeMap::new(),
        )?;
        if !prepared.image_pins.is_empty() {
            let mut observations = BTreeMap::new();
            for pin in &prepared.image_pins {
                let captured = process::capture_in(
                    &source,
                    "docker",
                    &["image", "inspect", "--format", "{{.Id}}", &pin.reference],
                    &[],
                    Duration::from_secs(5),
                    &cancellation,
                );
                output.atomic(
                    &format!("image-{}.log", pin.name),
                    &[
                        captured.stdout.as_slice(),
                        b"\n--- stderr ---\n",
                        &captured.stderr,
                    ]
                    .concat(),
                )?;
                if matches!(captured.exit, CaptureExit::Success) && !captured.truncated {
                    observations.insert(
                        pin.reference.clone(),
                        String::from_utf8(captured.stdout)?.trim().to_owned(),
                    );
                }
            }
            prepared = Preparation::prepare(
                &policy,
                &options.candidate,
                tree,
                &paths,
                options.cadence,
                &observations,
            )?;
        }
        candidate.run_checked(|_| Ok(()))?;
        resolved.snapshot.run_checked(|_| policy.verify())?;
        if fs::read(regular(
            anchor_path.parent().ok_or("anchor parent missing")?,
            anchor_path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or("anchor filename invalid")?,
        )?)? != anchor_bytes
        {
            return Err("launcher anchor changed during resolution".into());
        }
        output.atomic(
            "closure.json",
            &serde_json::to_vec_pretty(policy.closure())?,
        )?;
        cancel_preparation(&mut prepared, &cancellation);
        Ok(prepared)
    })();
    let (value, success) = match result {
        Ok(mut prepared) => {
            cancel_preparation(&mut prepared, &cancellation);
            let success = matches!(prepared.status, Status::PreparedNonAuthoritative);
            (
                serde_json::json!({"preparation":prepared,"limits":limits(),"comparison":"unavailable: conservatively selected all protected suites"}),
                success,
            )
        }
        Err(error) => (
            serde_json::json!({"schema":1,"authoritative":false,"status":"UNAVAILABLE","candidate_commit":options.candidate,"reasons":[error.to_string()],"limits":limits()}),
            false,
        ),
    };
    output.atomic("preparation.json", &serde_json::to_vec_pretty(&value)?)?;
    println!("{}", serde_json::to_string_pretty(&value)?);
    if !success {
        return Err("protected preparation unavailable; no candidate-policy fallback".into());
    }
    Ok(())
}
fn cancel_preparation(prepared: &mut Preparation, cancellation: &Cancellation) {
    if cancellation.cancelled() {
        prepared.status = Status::Unavailable;
        prepared
            .reasons
            .push("human cancellation; preparation not ready".into());
    }
}
fn limits() -> Vec<&'static str> {
    vec![
        "preparation executes no validation and is never authoritative",
        "bootstrap PATH/credentials and same-user files are not authenticated supervisor isolation",
        "protected migration requires human merge; candidate ABI cannot replace judge",
        "Git raw export/endpoint probes are not wall-supervised and cannot detect reverted races",
        "local raw Git helpers may inherit non-Git credential variables but perform no network commands",
        "image inspection binds preparation observations only, not workload-use attestations",
        "no restricted candidate execution, verified judge artifact, or Docker-daemon cleanup yet",
    ]
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn late_cancel_invalidates_ready_preparation_before_publication() {
        let source = SourceIdentity {
            repository: "Anko59/AoeWorld".into(),
            repository_id: 42,
            remote_url: "https://github.com/Anko59/AoeWorld.git".into(),
            protected_ref: "refs/heads/dev".into(),
            commit: "a".repeat(40),
            tree: "b".repeat(40),
            observed_at_unix_s: 1,
        };
        let mut prepared = Preparation {
            schema: 1,
            authoritative: false,
            status: Status::PreparedNonAuthoritative,
            source,
            closure_blake3: "c".repeat(64),
            candidate_commit: "d".repeat(40),
            candidate_tree: "e".repeat(40),
            canonical_registry_hash: None,
            gates: vec![],
            image_pins: vec![],
            reasons: vec![],
        };
        let cancellation = Cancellation::default();
        cancellation.cancel();
        cancel_preparation(&mut prepared, &cancellation);
        assert!(matches!(prepared.status, Status::Unavailable));
        assert!(!prepared.authoritative);
    }
}

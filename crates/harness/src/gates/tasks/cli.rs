use super::*;
use crate::{
    gates::{
        runner::{evidence::PrivateOutput, signals::Signals},
        scopes::{Kind, Snapshot},
    },
    process::{self, Cancellation, CaptureExit},
};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
#[derive(clap::Args)]
pub(crate) struct Options {
    /// Absolute existing bounded JSON task contract, treated as data not authority.
    #[arg(long)]
    task: PathBuf,
    /// Absolute existing external directory, including referenced task artifacts.
    #[arg(long)]
    output: PathBuf,
}
pub(crate) fn execute(root: &Path, options: Options) -> Result<()> {
    let root = fs::canonicalize(root)?;
    let output = PrivateOutput::new(&options.output, std::slice::from_ref(&root))?;
    // Check resolved aliases before replacing reserved outputs: the task input
    // itself may be reached through a directory symlink into the output directory.
    if options.task.starts_with(output.directory())
        || fs::canonicalize(&options.task).is_ok_and(|path| path.starts_with(output.directory()))
    {
        return Err("task input must not overlap output".into());
    }
    output.atomic(
        "task-plan.json",
        b"{\"authoritative\":false,\"status\":\"UNAVAILABLE\",\"reason\":\"planning in progress\"}",
    )?;
    let cancellation = Cancellation::default();
    let signals = Signals::new(&cancellation);
    let result = (|| -> Result<serde_json::Value> {
        signals.as_ref().map_err(|error| error.to_string())?;
        let input = io::file(&options.task, 32768)?;
        let task = Task::parse(&input)?;
        let candidate = Snapshot::prepare(&root, Kind::Commit(task.candidate.clone()))?;
        let base = Snapshot::prepare(&root, Kind::Commit(task.base.clone()))?;
        let diff = candidate.diff_from(&base, &cancellation)?;
        let registry = candidate.run_checked(Registry::load)?;
        let catalog_bytes =
            candidate.run_checked(|root| io::file(&root.join("gates/roles.json"), 16384))?;
        let catalog = Catalog::parse(&catalog_bytes)?;
        let hunks = diff
            .files
            .iter()
            .map(|file| integrity::Hunk {
                path: file.path.clone(),
                removed: file.removed.clone(),
                added: file.added.clone(),
            })
            .collect::<Vec<_>>();
        let provider = task.provider;
        let executable = match provider {
            adapters::Provider::Codex => "codex",
            adapters::Provider::PiDev => "pi",
            adapters::Provider::DeepSeekHarness => "dsh",
        };
        let observed = process::capture_in(
            &root,
            executable,
            &["--version"],
            &[],
            Duration::from_secs(5),
            &cancellation,
        );
        output.atomic(
            "provider-probe.log",
            &[
                observed.stdout.as_slice(),
                b"\n--- stderr ---\n",
                &observed.stderr,
            ]
            .concat(),
        )?;
        let version = std::str::from_utf8(&observed.stdout)
            .ok()
            .map(str::trim)
            .filter(|version| {
                !version.is_empty()
                    && version.len() <= 1024
                    && !version.chars().any(char::is_control)
            });
        let runtime = match (matches!(observed.exit, CaptureExit::Success) && !observed.truncated, version) {
            (true, Some(version)) => adapters::Observation::Installed { executable:executable.into(), version:version.into() },
            _ => adapters::Observation::Unavailable("fixed executable/version probe absent or invalid; provider-neutral planning remains available".into()),
        };
        let planned = plan(task, &registry, &catalog, &diff.paths, &hunks, runtime)?;
        let mut guides = Vec::new();
        for path in &planned.context.guides {
            let bytes = candidate.run_checked(|root| io::file(&root.join(path), 32768))?;
            std::str::from_utf8(&bytes)?;
            guides.push(serde_json::json!({"path":path,"blake3":blake3::hash(&bytes).to_hex().to_string(),"bytes":bytes.len()}));
        }
        let mut artifacts = Vec::new();
        for artifact in &planned.task.artifacts {
            let bytes = io::file(&output.directory().join(&artifact.path), 1024 * 1024)?;
            let digest = blake3::hash(&bytes).to_hex().to_string();
            if artifact.blake3.as_ref() != Some(&digest) {
                return Err("artifact must have a matching actual full BLAKE3 digest".into());
            }
            artifacts.push(
                serde_json::json!({"path":artifact.path,"kind":artifact.kind,"blake3":digest}),
            );
        }
        let context = serde_json::json!({"packet":planned.context,"guides":guides,"candidate_tree":candidate.identity.tree,"base_tree":base.identity.tree,"catalog_hash":planned.catalog_hash,"registry_hash":planned.registry_hash,"limits":"References and digests only; no prompt permissions, artifact verdict authentication or provider enforcement"});
        if serde_json::to_vec(&context)?.len() > 16384 {
            return Err("complete context exceeds 16KiB; split task".into());
        }
        candidate.run_checked(|_| Ok(()))?;
        base.run_checked(|_| Ok(()))?;
        if io::file(&options.task, 32768)? != input {
            return Err("task contract changed during planning".into());
        }
        if cancellation.cancelled() {
            return Err("task planning cancelled before publication".into());
        }
        output.atomic("task-diff.json", &serde_json::to_vec_pretty(&diff)?)?;
        output.atomic("task-context.json", &serde_json::to_vec_pretty(&context)?)?;
        Ok(
            serde_json::json!({"status":"PLANNED_NON_AUTHORITATIVE","authoritative":false,"plan":planned,"candidate_tree":candidate.identity.tree,"base_tree":base.identity.tree,"input_blake3":blake3::hash(&input).to_hex().to_string(),"artifacts":artifacts,"binary_changes_require_review":diff.files.iter().any(|file| file.binary),"limits":["candidate bootstrap and candidate catalog are feedback, not protected judge policy","task status/role/artifact JSON are unauthenticated claims; ready-for-human never approves merge","read-only Git diff probes are not wall-supervised; endpoints cannot detect reverted races","provider PATH/version observations do not establish SDK hooks, filesystem isolation or authenticated roles"]}),
        )
    })();
    let (value, success) = match result {
        Ok(value) if !cancellation.cancelled() => (value, true),
        Ok(_) => (
            serde_json::json!({"status":"UNAVAILABLE","authoritative":false,"reason":"planning cancelled before publication"}),
            false,
        ),
        Err(error) => (
            serde_json::json!({"status":"UNAVAILABLE","authoritative":false,"reason":error.to_string()}),
            false,
        ),
    };
    output.atomic("task-plan.json", &serde_json::to_vec_pretty(&value)?)?;
    println!("{}", serde_json::to_string_pretty(&value)?);
    if !success {
        return Err("task planning incomplete; see retained descriptor".into());
    }
    Ok(())
}

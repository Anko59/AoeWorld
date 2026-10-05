use super::*;
use crate::{
    gates::scopes::{Kind, Snapshot},
    process::Cancellation,
};
use evidence::{EndpointProof, JudgeIdentity, Metadata, PrivateOutput};
use std::time::Instant;

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub(crate) enum Scope {
    Working,
    Index,
    Commit,
}
#[derive(clap::Args)]
pub(crate) struct Options {
    #[arg(long, value_enum, default_value = "edit", conflicts_with = "job")]
    cadence: Cadence,
    #[arg(long)]
    job: Option<String>,
    #[arg(long, value_enum, default_value = "working")]
    scope: Scope,
    #[arg(long)]
    revision: Option<String>,
    /// Existing, external, launcher-owned directory. Never inside a writable input/cache mount.
    #[arg(long)]
    output: PathBuf,
    #[arg(long, default_value_t = 7200)]
    total_seconds: u64,
    #[arg(long, default_value_t = 3600)]
    per_gate_seconds: u64,
}

pub(crate) fn execute(root: &Path, options: Options) -> Result<()> {
    let started = Instant::now();
    let total = Duration::from_secs(options.total_seconds);
    let cancellation = Cancellation::default();
    let _signals = signals::Signals::new(&cancellation)?;
    let source = fs::canonicalize(root)?;
    let kind = match (options.scope, options.revision) {
        (Scope::Working, None) => Kind::Working,
        (Scope::Index, None) => Kind::Index,
        (Scope::Commit, Some(commit)) => Kind::Commit(commit),
        _ => return Err("--revision must be supplied only with --scope commit".into()),
    };
    let git_common = crate::hooks::common_directory(&source)?;
    let cargo = source.join(".cache/cargo");
    let target = source.join("target");
    fs::create_dir_all(&cargo)?;
    fs::create_dir_all(&target)?;
    let output = PrivateOutput::new(
        &options.output,
        &[source.clone(), git_common, cargo, target],
    )?;
    eprintln!(
        "bootstrap-local evidence directory: {}",
        output.directory().display()
    );
    let snapshot = Snapshot::prepare(&source, kind)?;
    let root = ExecutionRoot::new(snapshot.root())?;
    let registry = Registry::load(snapshot.root())?;
    let classification = registry.classify(&snapshot.paths);
    let selection = options.job.as_deref().map_or(
        Selection::Cadence {
            cadence: options.cadence,
            suites: &classification.suites,
        },
        Selection::CiJob,
    );
    let plan = PreparedPlan::new(&registry, selection)?;
    let (source_fingerprint, private) = snapshot.fingerprints()?;
    let fingerprint = EndpointProof {
        source: source_fingerprint,
        private,
    };
    let executable = std::env::current_exe()?;
    let metadata = Metadata {
        revision: match &snapshot.identity.kind { Kind::Commit(commit) => commit.clone(), _ => snapshot.identity.source_head.clone() },
        base_revision: None, tree: snapshot.identity.tree.clone(), scope: snapshot.identity.kind.clone(),
        index_fingerprint: snapshot.identity.index_fingerprint.clone(), fingerprint,
        judge: JudgeIdentity { executable_blake3: blake3::hash(&fs::read(executable)?).to_hex().to_string(), policy_revision: None, trust_closure_hash: None, mode: "bootstrap-local".into() },
        tool_image_actual_ids: BTreeMap::new(),
        capability_limits: vec!["source-art/geodata/hardware qualification not supplied".into()],
        runtime_limits: vec!["candidate bootstrap and Make are arbitrary code; same-user evidence forging possible".into(), "no installed provider interception or hostile isolation claimed".into(), "Git endpoint probes cannot detect reverted edits and are not wall-supervised".into(), "process groups do not stop escaped or Docker-daemon work; no daemon cleanup claimed".into(), "image IDs are post-execution observations, not proof of which images candidate Make used".into()],
    };
    let mut runtime = real::Local {
        snapshot: &snapshot,
        source: source.clone(),
        output: &output,
        cancellation,
        started,
        total,
    };
    let mut ledger = run(
        &mut runtime,
        &root,
        &plan,
        metadata,
        Budgets {
            total: total.saturating_sub(started.elapsed()),
            per_gate_max: Duration::from_secs(options.per_gate_seconds),
        },
    );
    ledger.metadata.tool_image_actual_ids = real::images(
        &source,
        &runtime.cancellation,
        total.saturating_sub(started.elapsed()),
    );
    if !triage::probe(
        &mut runtime,
        &root,
        &ledger.metadata.fingerprint,
        triage::Phase::FinalCliAfterImages,
        None,
        &mut ledger.endpoints,
        &mut ledger.invalid_reasons,
    ) {
        ledger.overall = Overall::Invalid;
    }
    ledger.duration_ms = started.elapsed().as_millis().try_into().unwrap_or(u64::MAX);
    ledger.budgets.total = total;
    if ledger.overall == Overall::Pass && (runtime.cancelled() || started.elapsed() > total) {
        ledger.overall = Overall::Incomplete;
        ledger
            .invalid_reasons
            .push("cancelled or overall budget exhausted including metadata work".into());
    }
    publish(&output, &ledger)
}

/// Publish local raw evidence separately; always emit this run's safe facts.
pub(super) fn publish(output: &PrivateOutput, ledger: &evidence::Ledger) -> Result<()> {
    let publication = output.ledger(ledger);
    let status = match &publication {
        Ok(_) => triage::Publication::Published,
        Err(error) => triage::Publication::Failed {
            io_kind: triage::io_kind(error.as_ref()),
        },
    };
    println!("{}", triage::summary(ledger, &status)?);
    match publication {
        Ok(path) => eprintln!("local UNSANITIZED ledger: {}", path.display()),
        Err(_) => {
            return Err(format!(
                "final local publication {status:?}; execution {:?}",
                ledger.overall
            )
            .into());
        }
    }
    if ledger.overall != Overall::Pass {
        return Err(format!(
            "gate execution {:?}; see retained complete ledger",
            ledger.overall
        )
        .into());
    }
    Ok(())
}

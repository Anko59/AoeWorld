//! Making the servers match dev: the decision, the start, the lock that keeps
//! two sessions from building at once.
use super::{
    build::{self, Built},
    layout::Layout,
    presets, process, revision,
    state::{Instance, State, now},
    warm,
};
use nix::fcntl::{Flock, FlockArg};
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub(crate) const PLAY: &str = "play";
const START_DEADLINE: Duration = Duration::from_secs(60);
/// View servers kept besides the play server; the oldest is stopped past it.
const MAX_VIEWS: usize = 3;

#[cfg(test)]
#[path = "tests/ready.rs"]
mod tests;

/// The settings of one command, read once from `AOE_DEV_SERVER_*`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Settings {
    /// Any commit-ish to serve instead of `origin/dev` (proofs, bisecting).
    pub(crate) rev: Option<String>,
    /// The play server's port; view servers take the next eight.
    pub(crate) port: u16,
    /// More `<preset>:<size>` maps to keep prepared.
    pub(crate) presets: Option<String>,
    pub(crate) pack: Option<String>,
    pub(crate) fetch: bool,
}

impl Settings {
    pub(crate) fn from_env(
        get: &dyn Fn(&str) -> Option<String>,
    ) -> std::result::Result<Self, String> {
        let var = |name: &str| get(name).filter(|v| !v.is_empty());
        let port = match var("AOE_DEV_SERVER_PORT") {
            None => 8091,
            Some(text) => text
                .parse::<u16>()
                .ok()
                .filter(|p| *p >= 1024 && *p <= 65_500 && text.bytes().all(|b| b.is_ascii_digit()))
                .ok_or_else(|| {
                    format!("AOE_DEV_SERVER_PORT={text}: use a port from 1024 to 65500")
                })?,
        };
        Ok(Self {
            rev: var("AOE_DEV_SERVER_REV"),
            port,
            presets: var("AOE_DEV_SERVER_PRESETS"),
            pack: var("AOE_DEV_SERVER_PACK"),
            fetch: var("AOE_DEV_SERVER_FETCH").as_deref() != Some("0"),
        })
    }

    pub(crate) fn current() -> std::result::Result<Self, String> {
        Self::from_env(&|name| std::env::var(name).ok())
    }
}

/// What to do about one instance.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Action {
    Keep,
    Start,
    /// Stop it and start it again, for this reason.
    Restart(String),
}

/// `current` is the recorded instance, `alive` its process, `build` what its
/// port reports. It is kept only if all three agree with what is wanted.
pub(crate) fn decide(
    current: Option<&Instance>,
    alive: bool,
    build: Option<&str>,
    revision: &str,
    identity: &str,
) -> Action {
    let Some(instance) = current else {
        return Action::Start;
    };
    if !alive {
        return Action::Restart("its process is gone".into());
    }
    match build {
        None => Action::Restart("it does not answer /health".into()),
        Some(served) if served != revision => Action::Restart(format!(
            "it serves {} and dev is at {}",
            short(served),
            short(revision)
        )),
        Some(_) if instance.identity != identity => {
            Action::Restart("its configuration changed".into())
        }
        Some(_) => Action::Keep,
    }
}

pub(crate) fn short(revision: &str) -> &str {
    revision.get(..12).unwrap_or(revision)
}

/// Held while building or starting; the kernel drops it with the process.
pub(crate) struct Held(#[allow(dead_code)] Flock<File>);

pub(crate) fn lock(layout: &Layout) -> Result<Held> {
    fs::create_dir_all(&layout.home)?;
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(layout.lock())?;
    match Flock::lock(file, FlockArg::LockExclusiveNonblock) {
        Ok(flock) => Ok(Held(flock)),
        Err((file, _)) => {
            eprintln!("dev-server: another session is building or starting it; waiting");
            Flock::lock(file, FlockArg::LockExclusive)
                .map(Held)
                .map_err(|(_, e)| format!("{}: {e}", layout.lock().display()).into())
        }
    }
}

fn pack(layout: &Layout, root: &Path, settings: &Settings) -> Result<(PathBuf, String)> {
    let assets = layout
        .shared(root, "local-assets")
        .ok_or("no local-assets in this checkout or the main one (make assets-import)")?;
    let base = assets.parent().unwrap_or(&assets);
    let name = crate::ship::compare::config::asset_pack(base, settings.pack.as_deref())?;
    Ok((assets, name))
}

fn link_assets(tree: &Path, assets: &Path) -> Result<()> {
    let link = tree.join("local-assets");
    if fs::symlink_metadata(&link).is_err() {
        std::os::unix::fs::symlink(assets, link)?;
    }
    Ok(())
}

fn wait_healthy(
    port: u16,
    revision: &str,
    pid: u32,
    container: Option<&str>,
    binary: &Path,
) -> Result<()> {
    let deadline = Instant::now() + START_DEADLINE;
    while Instant::now() < deadline {
        if process::health_build(port).as_deref() == Some(revision) {
            return Ok(());
        }
        let up = container.map_or_else(|| process::running(pid, binary), process::docker_running);
        if !up {
            return Err("the server exited while starting; see its log".into());
        }
        thread::sleep(Duration::from_millis(100));
    }
    Err(format!(
        "the server was not healthy on port {port} within {}s",
        START_DEADLINE.as_secs()
    )
    .into())
}

/// Start (or restart) one instance and record it.
#[allow(clippy::too_many_arguments)]
fn bring_up(
    layout: &Layout,
    built: &Built,
    state: &mut State,
    name: &str,
    port: u16,
    revision: &str,
    identity: String,
    environment: Vec<(String, String)>,
    mounts: Option<Vec<process::Mount>>,
) -> Result<Instance> {
    if let Some(old) = state.find(name).cloned() {
        old.halt(layout);
    }
    fs::create_dir_all(layout.logs())?;
    let mut all = vec![
        ("AOE_BIND".to_owned(), format!("127.0.0.1:{port}")),
        ("AOE_SCENARIO".to_owned(), "smoke".to_owned()),
        ("AOE_BUILD_SHA".to_owned(), revision.to_owned()),
    ];
    all.extend(environment);
    let (pid, container) = match mounts {
        Some(mounts) => {
            let container = format!("aoeworld-dev-server-{name}");
            all.retain(|(key, _)| key != "AOE_BIND");
            process::docker_run(&container, port, &built.tree, &built.server, &all, &mounts)?;
            (0, Some(container))
        }
        None => {
            let log = layout.logs().join(format!("{name}.log"));
            let pid = process::spawn(&built.server, &[], &built.tree, &all, &log)?;
            (pid, None)
        }
    };
    wait_healthy(port, revision, pid, container.as_deref(), &built.server)?;
    let instance = Instance {
        name: name.to_owned(),
        port,
        pid,
        started: now(),
        revision: revision.to_owned(),
        identity,
        container,
    };
    state.replace(instance.clone());
    state.write(&layout.state())?;
    Ok(instance)
}

impl Instance {
    /// The executable the instance was started from.
    pub(crate) fn binary(&self, layout: &Layout) -> PathBuf {
        Built::of(layout, &self.revision).server
    }

    pub(crate) fn alive(&self, layout: &Layout) -> bool {
        match &self.container {
            Some(name) => process::docker_running(name),
            None => process::running(self.pid, &self.binary(layout)),
        }
    }

    pub(crate) fn halt(&self, layout: &Layout) {
        match &self.container {
            Some(name) => process::docker_stop(name),
            None => process::stop(self.pid, &self.binary(layout)),
        }
    }
}

/// Make the play server current: build the revision if needed, keep or
/// restart the process, keep the prepared maps of an unchanged fingerprint.
/// Returns whether the running server was kept as it was.
pub(crate) fn ensure_play(root: &Path, settings: &Settings) -> Result<bool> {
    let layout = Layout::resolve(root)?;
    let _held = lock(&layout)?;
    if settings.fetch
        && settings.rev.is_none()
        && let Some(why) = revision::fetch(root)
    {
        eprintln!("dev-server: could not fetch origin/dev ({why}); using the ref it has");
    }
    let revision = revision::expected(root, settings.rev.as_deref())?;
    let built = build::ensure(&layout, root, &revision)?;
    let (assets, pack) = pack(&layout, root, settings)?;
    link_assets(&built.tree, &assets)?;
    let fingerprint = revision::fingerprint(root, &revision)?;
    let directory = layout.presets(&fingerprint);
    let packages = directory.join("map-packages");
    fs::create_dir_all(&packages)?;
    let bookmarks = fs::read_to_string(root.join(crate::ship::compare::DEFAULT_BOOKMARKS)).ok();
    let wanted = presets::wanted(bookmarks.as_deref(), settings.presets.as_deref())?;
    let file = directory.join("presets.json");
    fs::write(&file, serde_json::to_vec_pretty(&wanted)?)?;
    let geodata = layout
        .shared(root, ".cache/geodata")
        .ok_or("no .cache/geodata in this checkout or the main one (make geodata-bootstrap)")?;
    let identity = format!(
        "container:{}:{pack}:{}:{}",
        fingerprint,
        blake3::hash(&serde_json::to_vec(&wanted)?).to_hex()[..16].to_owned(),
        geodata.display()
    );
    let mut state = State::read(&layout.state());
    let current = state.find(PLAY).cloned();
    let alive = current.as_ref().is_some_and(|i| i.alive(&layout));
    let build_now = current.as_ref().and_then(|i| process::health_build(i.port));
    let action = decide(
        current.as_ref(),
        alive,
        build_now.as_deref(),
        &revision,
        &identity,
    );
    let kept = action == Action::Keep;
    match (action, current) {
        (Action::Keep, Some(_)) => {}
        (action, _) => {
            if let Action::Restart(why) = &action {
                eprintln!("dev-server: restarting the play server: {why}");
            }
            let environment = vec![
                (
                    "AOE_ASSET_PACK".into(),
                    format!("local-assets/packs/{pack}"),
                ),
                ("AOE_MAP_WORKER".into(), built.worker.display().to_string()),
                ("AOE_GEODATA_CACHE".into(), geodata.display().to_string()),
                (
                    "AOE_MAP_PACKAGE_DIRECTORY".into(),
                    packages.display().to_string(),
                ),
                ("AOE_PRESET_MAPS".into(), file.display().to_string()),
            ];
            bring_up(
                &layout,
                &built,
                &mut state,
                PLAY,
                settings.port,
                &revision,
                identity,
                environment,
                Some(vec![
                    process::Mount {
                        path: built.tree.clone(),
                        writable: false,
                    },
                    process::Mount {
                        path: assets.canonicalize()?,
                        writable: false,
                    },
                    process::Mount {
                        path: directory.clone(),
                        writable: false,
                    },
                    process::Mount {
                        path: packages.clone(),
                        writable: true,
                    },
                    process::Mount {
                        path: geodata.canonicalize()?,
                        writable: true,
                    },
                ]),
            )?;
        }
    }
    retire(&layout, &mut state, &fingerprint);
    warm_presets(settings.port, &wanted);
    Ok(kept)
}

/// "Maps ready" means a start can use them: wait for the presets the server
/// prepares at boot (bounded) and serve each one's minimap raster, so the first
/// skirmish does not pay for it under the server's 30 s budget (warm.rs). A
/// failure is reported, not fatal: the server itself is up.
fn warm_presets(port: u16, wanted: &[serde_json::Value]) {
    warm_presets_for(port, wanted, warm::BUDGET);
}

fn warm_presets_for(port: u16, wanted: &[serde_json::Value], budget: Duration) {
    let deadline = Instant::now() + budget;
    let mut warmed = BTreeSet::new();
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            eprintln!(
                "dev-server: warning: maps are still being prepared or warmed after {} s",
                budget.as_secs()
            );
            return;
        }
        let jobs = process::http_get_for(port, "/maps/jobs", left).unwrap_or_default();
        let completed: Vec<_> = wanted
            .iter()
            .map(|want| warm::completed(&jobs, want))
            .collect();
        if let Some(hash) = completed
            .iter()
            .flatten()
            .find(|hash| !warmed.contains(*hash))
        {
            match warm::raster(
                port,
                hash,
                deadline.saturating_duration_since(Instant::now()),
            ) {
                Ok(took) => {
                    eprintln!(
                        "dev-server: map {} ready ({} ms)",
                        &hash[..12.min(hash.len())],
                        took.as_millis()
                    );
                    warmed.insert(hash.clone());
                }
                Err(why) => {
                    eprintln!("dev-server: warning: {why}");
                    return;
                }
            }
            // A different job may have completed during this request. Read
            // again before warming another map or declaring the set ready.
            continue;
        }
        if completed.iter().all(|hashes| !hashes.is_empty()) {
            return;
        }
        thread::sleep(
            Duration::from_secs(5).min(deadline.saturating_duration_since(Instant::now())),
        );
    }
}

/// Drop what nothing uses any more: trees of revisions no instance serves
/// and prepared maps of other fingerprints.
fn retire(layout: &Layout, state: &mut State, fingerprint: &str) {
    let served: Vec<&str> = state
        .instances
        .iter()
        .map(|i| i.revision.as_str())
        .collect();
    build::prune_trees(layout, &served);
    build::prune_presets(layout, &[fingerprint]);
}

/// A view server (one map open) at `revision`, shared by every run that asks
/// for the same map, packages and pack. Returns its URL.
pub(crate) fn ensure_view(
    root: &Path,
    settings: &Settings,
    revision_wanted: &str,
    pack: &str,
    packages: &Path,
    open: &str,
) -> Result<String> {
    let layout = Layout::resolve(root)?;
    let _held = lock(&layout)?;
    let built = build::ensure(&layout, root, revision_wanted)?;
    let (assets, _) = self::pack(&layout, root, settings)?;
    link_assets(&built.tree, &assets)?;
    let packages = packages.canonicalize()?;
    let identity = format!("{open}:{}:{pack}", packages.display());
    let name = format!("view-{}", &blake3::hash(identity.as_bytes()).to_hex()[..10]);
    let mut state = State::read(&layout.state());
    let current = state.find(&name).cloned();
    let alive = current.as_ref().is_some_and(|i| i.alive(&layout));
    let build_now = current.as_ref().and_then(|i| process::health_build(i.port));
    let action = decide(
        current.as_ref(),
        alive,
        build_now.as_deref(),
        revision_wanted,
        &identity,
    );
    if let (Action::Keep, Some(instance)) = (&action, &current) {
        return Ok(format!("http://127.0.0.1:{}/", instance.port));
    }
    let views: Vec<Instance> = state
        .instances
        .iter()
        .filter(|i| i.name.starts_with("view-") && i.name != name)
        .cloned()
        .collect();
    if views.len() >= MAX_VIEWS
        && let Some(oldest) = views.iter().min_by_key(|i| i.started)
    {
        oldest.halt(&layout);
        state.instances.retain(|i| i.name != oldest.name);
    }
    let taken: Vec<u16> = state.instances.iter().map(|i| i.port).collect();
    let port = current
        .as_ref()
        .map(|i| i.port)
        .or_else(|| process::free_port(settings.port + 1, &taken))
        .ok_or("no free port for another dev-server view")?;
    let environment = crate::ship::compare::server::side_env(pack, &packages, open)
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect();
    let instance = bring_up(
        &layout,
        &built,
        &mut state,
        &name,
        port,
        revision_wanted,
        identity,
        environment,
        None,
    )?;
    Ok(format!("http://127.0.0.1:{}/", instance.port))
}

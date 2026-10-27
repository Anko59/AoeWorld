//! One running background bake and at most eight package requests. Weak
//! installation targets cancel retired worlds and share one result with peers.
use super::*;
use aoe_map::NavigationTarget;
use std::{
    collections::BTreeMap,
    sync::{LazyLock, Mutex},
};

type Key = (Option<PathBuf>, [u8; 32]);
static JOBS: LazyLock<Mutex<BTreeMap<Key, Vec<NavigationTarget>>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));
static WORKER: Mutex<()> = Mutex::new(());
const MAX_PACKAGES: usize = 8;
const MAX_TARGETS: usize = 256;

fn interested(key: &Key) -> bool {
    JOBS.lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(key)
        .is_some_and(|targets| targets.iter().any(NavigationTarget::is_alive))
}

pub(super) fn launch(generator: &MapChunkGenerator, hash: [u8; 32], directory: Option<&Path>) {
    let key = (directory.map(Path::to_path_buf), hash);
    let mut jobs = JOBS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(targets) = jobs.get_mut(&key) {
        targets.retain(NavigationTarget::is_alive);
        if targets.len() < MAX_TARGETS {
            targets.push(generator.navigation_target());
        }
        return;
    }
    if jobs.len() >= MAX_PACKAGES {
        tracing::warn!("navigation preparation queue full; lazy navigation remains active");
        return;
    }
    jobs.insert(key.clone(), vec![generator.navigation_target()]);
    drop(jobs);
    let worker = generator.navigation_preparation_worker();
    let task_key = key.clone();
    if let Err(error) = std::thread::Builder::new().name("package-navigation".into()).spawn(move || {
        let _slot = WORKER.lock().unwrap_or_else(|e| e.into_inner());
        let result = prepare(&worker, hash, task_key.0.as_deref(), &|| !interested(&task_key));
        let targets = JOBS.lock().unwrap_or_else(|e| e.into_inner()).remove(&task_key).unwrap_or_default();
        match result {
            Ok(()) => if let Some(snapshot) = worker.navigation_snapshot() {
                for target in targets {
                    if let Err(error) = target.install(snapshot.clone()) { tracing::warn!(%error, "navigation installation failed"); }
                }
            },
            Err(error) => tracing::warn!(%error, "navigation bake unavailable; lazy navigation remains active"),
        }
    }) {
        JOBS.lock().unwrap_or_else(|e| e.into_inner()).remove(&key);
        tracing::warn!(%error, "navigation worker unavailable; lazy navigation remains active");
    }
}

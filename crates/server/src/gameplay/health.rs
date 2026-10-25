use super::*;

#[derive(Serialize)]
pub(super) struct Health {
    status: &'static str,
    build: String,
    scenario: String,
    tick: u64,
    entities: usize,
    loaded_chunks: usize,
    tick_deadline_misses: u64,
    terrain_cache: terrain_cache::Usage,
    navigation_cache: NavigationCacheHealth,
    gameplay_memory: gameplay::probe::MemoryUsage,
    snapshot_store: map_store::navigation::SnapshotUsage,
}

#[derive(Serialize)]
struct NavigationCacheHealth {
    entries: usize,
    /// Logical cached-outcome bytes (no allocator or BTreeMap overhead): a budget, not RSS.
    retained_bytes: usize,
    /// Logical cached-outcome budget rather than a hard process-memory cap.
    limit_bytes: usize,
}

impl From<aoe_simulation::NavigationCacheUsage> for NavigationCacheHealth {
    fn from(value: aoe_simulation::NavigationCacheUsage) -> Self {
        Self {
            entries: value.entries,
            retained_bytes: value.retained_bytes,
            limit_bytes: value.limit_bytes,
        }
    }
}

pub(super) async fn health(State(state): State<AppState>) -> Json<Health> {
    let terrain_cache = state.terrain_cache.lock().await.usage();
    let gameplay = state.gameplay.read().await.clone();
    let navigation_cache = gameplay.navigation_cache_usage().await.into();
    let gameplay_memory = gameplay.memory_usage().await;
    let snapshot_store = map_store::navigation::snapshot_usage();
    state.ensure_diagnostic_world().await;
    let world_guard = state.world.read().await;
    let Some(world) = world_guard.as_ref() else {
        return Json(Health {
            status: "unavailable",
            build: state.build.to_string(),
            scenario: state.diagnostic_scenario.name.to_owned(),
            tick: 0,
            entities: 0,
            loaded_chunks: 0,
            tick_deadline_misses: state.tick_deadline_misses(),
            terrain_cache,
            navigation_cache,
            gameplay_memory,
            snapshot_store,
        });
    };
    Json(Health {
        status: "ok",
        build: state.build.to_string(),
        scenario: world.scenario().name.to_owned(),
        tick: world.tick().0,
        entities: world.entities().len(),
        loaded_chunks: world.loaded_chunks(),
        tick_deadline_misses: state.tick_deadline_misses(),
        terrain_cache,
        navigation_cache,
        gameplay_memory,
        snapshot_store,
    })
}

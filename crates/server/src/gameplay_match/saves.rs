//! Saved games: the person saves a running match from the game menu and
//! resumes it later from the main menu. A save is a file under `saves/` beside
//! the map package directory (ignored by Git): the match's seats and a
//! `WorldSnapshot`, with a small JSON card next to it for listing. Resuming
//! builds the same map, restores the snapshot into it, and starts the AI
//! opponents again with the ordinary skirmish machinery.
use super::{MatchError, MatchRules, skirmish, warmup};
use crate::{AppState, GameplayService};
use aoe_core::PlayerId;
use aoe_map::{EnvironmentPageProvider, MapPackage};
use aoe_simulation::{GameWorld, GameWorldError, WorldSnapshot};
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path as FsPath, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

const SAVE_VERSION: u16 = 1;
const MAX_NAME: usize = 60;
const MAX_SAVE_BYTES: u64 = 2 << 30;

#[derive(Serialize, Deserialize)]
struct SaveSeat {
    primary_unit_id: u32,
    human: bool,
}

#[derive(Serialize, Deserialize)]
struct SaveFile {
    version: u16,
    seats: Vec<SaveSeat>,
    world: WorldSnapshot,
}

/// The listing card of one save.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct SaveCard {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) saved_at: u64,
    pub(crate) tick: u64,
    pub(crate) game_seconds: u64,
    pub(crate) map_hash: String,
    pub(crate) opponents: usize,
    pub(crate) units: usize,
    pub(crate) buildings: usize,
    pub(crate) bytes: u64,
    /// Milliseconds the snapshot, encoding and write took.
    pub(crate) save_ms: u64,
}

type Error = (StatusCode, String);

pub(crate) fn directory(state: &AppState) -> Option<PathBuf> {
    let packages = state.map_package_directory.as_deref()?;
    Some(packages.parent().unwrap_or(FsPath::new(".")).join("saves"))
}

fn slug(name: &str) -> String {
    let slug: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    slug.trim_matches('-').chars().take(24).collect()
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

impl GameplayService {
    /// Writes this match to `directory` as `name`.
    pub(crate) async fn save_game(
        &self,
        directory: PathBuf,
        name: String,
    ) -> Result<SaveCard, String> {
        let hash = self.map_hash().ok_or("this game has no map to save")?;
        let seats: Vec<SaveSeat> = self
            .slot_infos()
            .await
            .into_iter()
            .map(|slot| SaveSeat {
                primary_unit_id: slot.primary_unit_id.0,
                human: slot.human,
            })
            .collect();
        let guard = self.world.clone().read_owned().await;
        let hz = guard.config().tick_hz.max(1);
        tokio::task::spawn_blocking(move || {
            let started = std::time::Instant::now();
            let world = guard
                .snapshot(hash)
                .ok_or("this game's terrain cannot be saved")?;
            drop(guard);
            let (tick, hz) = (world.tick, u64::from(hz));
            let (units, buildings) = (world.units.len(), world.buildings.len());
            let opponents = seats.iter().filter(|seat| !seat.human).count();
            let bytes = postcard::to_allocvec(&SaveFile {
                version: SAVE_VERSION,
                seats,
                world,
            })
            .map_err(|error| error.to_string())?;
            std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
            let saved_at = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let id = format!("{saved_at}-{}", slug(&name));
            let id = id.trim_end_matches('-').to_owned();
            let temporary = directory.join(format!("{id}.save.tmp"));
            std::fs::write(&temporary, &bytes).map_err(|error| error.to_string())?;
            std::fs::rename(&temporary, directory.join(format!("{id}.save")))
                .map_err(|error| error.to_string())?;
            let card = SaveCard {
                id: id.clone(),
                name,
                saved_at,
                tick,
                game_seconds: tick / hz,
                map_hash: hash.iter().map(|byte| format!("{byte:02x}")).collect(),
                opponents,
                units,
                buildings,
                bytes: bytes.len() as u64,
                save_ms: started.elapsed().as_millis() as u64,
            };
            std::fs::write(
                directory.join(format!("{id}.json")),
                serde_json::to_vec(&card).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
            Ok(card)
        })
        .await
        .map_err(|_| "the save task failed".to_owned())?
    }
}

fn list(directory: &FsPath) -> Vec<SaveCard> {
    let mut cards: Vec<SaveCard> = std::fs::read_dir(directory)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|e| e == "json"))
        .filter_map(|entry| serde_json::from_slice(&std::fs::read(entry.path()).ok()?).ok())
        .filter(|card: &SaveCard| directory.join(format!("{}.save", card.id)).exists())
        .collect();
    cards.sort_by(|a, b| b.saved_at.cmp(&a.saved_at).then(a.id.cmp(&b.id)));
    cards
}

/// A save read back: what `Resume` needs before it builds the world.
pub(super) struct Loaded {
    file: SaveFile,
    pub(super) card: SaveCard,
}

pub(super) fn load_file(state: &AppState, id: &str) -> Result<Loaded, Error> {
    let missing = || (StatusCode::NOT_FOUND, "unknown saved game".to_owned());
    read_save(&directory(state).ok_or_else(missing)?, id)
}

fn read_save(directory: &FsPath, id: &str) -> Result<Loaded, Error> {
    let missing = || (StatusCode::NOT_FOUND, "unknown saved game".to_owned());
    if !valid_id(id) {
        return Err(missing());
    }
    let card: SaveCard = serde_json::from_slice(
        &std::fs::read(directory.join(format!("{id}.json"))).map_err(|_| missing())?,
    )
    .map_err(|_| missing())?;
    let path = directory.join(format!("{id}.save"));
    let length = std::fs::metadata(&path).map_err(|_| missing())?.len();
    if length > MAX_SAVE_BYTES {
        return Err((
            StatusCode::BAD_REQUEST,
            "the saved game is too large".to_owned(),
        ));
    }
    let file: SaveFile = postcard::from_bytes(&std::fs::read(&path).map_err(|_| missing())?)
        .map_err(|error| {
            (
                StatusCode::BAD_REQUEST,
                format!("the saved game is unreadable: {error}"),
            )
        })?;
    if file.version != SAVE_VERSION {
        return Err((
            StatusCode::BAD_REQUEST,
            "the saved game is from another version".to_owned(),
        ));
    }
    Ok(Loaded { file, card })
}

/// A request to resume a save takes its map and opponent count from the save.
pub(super) fn resolve(
    state: &AppState,
    save: &mut Option<String>,
    map_hash: &mut String,
    opponents: &mut usize,
) -> Result<Option<Arc<Loaded>>, Error> {
    let Some(id) = save.as_deref() else {
        return Ok(None);
    };
    let loaded = load_file(state, id)?;
    map_hash.clone_from(&loaded.card.map_hash);
    *opponents = loaded.opponents();
    Ok(Some(Arc::new(loaded)))
}

/// Makes the match of a skirmish: the saved one when `resume` is given, else
/// a new one with villages placed from `seed`.
pub(super) async fn build(
    package: MapPackage,
    provider: Option<Arc<dyn aoe_map::EnvironmentPageProvider>>,
    resume: Option<Arc<Loaded>>,
    humans: Vec<bool>,
    (seed, placed): (u64, Arc<std::sync::atomic::AtomicUsize>),
    cancelled: Arc<std::sync::atomic::AtomicBool>,
    directory: Option<PathBuf>,
) -> Result<GameplayService, String> {
    use std::sync::atomic::Ordering;
    tokio::task::spawn_blocking(move || {
        let cancelled = || cancelled.load(Ordering::Acquire);
        match resume {
            Some(loaded) => {
                resume_service(package, provider, &loaded, &cancelled, directory.as_deref()).map(
                    |(service, report)| {
                        tracing::info!(?report, "saved game restored");
                        service
                    },
                )
            }
            None => GameplayService::from_match_rules_stored(
                package,
                provider,
                &humans,
                &MatchRules::default().with_seed(seed),
                &placed,
                &cancelled,
                directory.as_deref(),
            )
            .map(|built| built.0),
        }
    })
    .await
    .map_err(|_| "match task failed".to_owned())?
    .map_err(|error| error.to_string())
}

impl Loaded {
    pub(super) fn opponents(&self) -> usize {
        self.card.opponents
    }
}

/// The saved match as a running service: the same map, the snapshot restored
/// into it, one seat per saved seat (fresh tokens).
fn resume_service(
    package: MapPackage,
    provider: Option<Arc<dyn EnvironmentPageProvider>>,
    loaded: &Loaded,
    cancelled: &dyn Fn() -> bool,
    directory: Option<&FsPath>,
) -> Result<(GameplayService, aoe_simulation::RestoreReport), MatchError> {
    let content_hash = package.content_hash;
    let content_hash_hex = package.content_hash_hex();
    let metadata = crate::gameplay_map::map_metadata(&package);
    let mut world = match provider {
        Some(provider) => GameWorld::from_page_provider(package, provider)?,
        None if package.environment.samples_per_axis == 0 => GameWorld::from_map(package)?,
        None => return Err(GameWorldError::InvalidTerrain.into()),
    };
    crate::map_store::navigation::prepare_world(&world, content_hash, directory, cancelled)
        .map_err(MatchError::Navigation)?;
    world
        .prepare_navigation(cancelled)
        .map_err(GameWorldError::Environment)?;
    let seats = &loaded.file.seats;
    world.set_limits(MatchRules::default().limits(seats.len()));
    let report = world
        .restore_snapshot(&loaded.file.world, content_hash, cancelled)
        .map_err(|error| match error {
            aoe_simulation::SnapshotError::World(error) => MatchError::World(error),
            _ => MatchError::World(GameWorldError::InvalidTerrain),
        })?;
    let starts: Vec<aoe_core::EntityId> = seats
        .iter()
        .map(|seat| aoe_core::EntityId(seat.primary_unit_id))
        .collect();
    let humans: Vec<bool> = seats.iter().map(|seat| seat.human).collect();
    let terrain = warmup::camera(&world, &starts, &humans, &content_hash_hex, cancelled)?;
    warmup::prepare(&world, &starts, cancelled)?;
    world
        .prepare_navigation(cancelled)
        .map_err(GameWorldError::Environment)?;
    let setups = starts
        .into_iter()
        .zip(humans)
        .enumerate()
        .map(
            |(index, (primary_unit_id, human))| crate::gameplay::SlotSetup {
                player: PlayerId(index as u16),
                primary_unit_id,
                human,
            },
        )
        .collect();
    let service =
        GameplayService::from_world_slots(world, setups, Some(content_hash), Some(metadata));
    service.install_initial_terrain(terrain);
    Ok((service, report))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SaveRequest {
    name: String,
}

async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<SaveRequest>,
) -> Result<Json<SaveCard>, Error> {
    skirmish::authorize(&state, &headers).await?;
    let name = request
        .name
        .trim()
        .chars()
        .take(MAX_NAME)
        .collect::<String>();
    if name.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "name the saved game".to_owned()));
    }
    let directory = directory(&state).ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "this server has no save directory".to_owned(),
    ))?;
    let service = state.gameplay.read().await.clone();
    let card = service
        .save_game(directory, name)
        .await
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?;
    tracing::info!(id = %card.id, bytes = card.bytes, ms = card.save_ms, "game saved");
    Ok(Json(card))
}

async fn index(State(state): State<AppState>) -> Json<Vec<SaveCard>> {
    Json(directory(&state).map(|d| list(&d)).unwrap_or_default())
}

async fn remove(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, Error> {
    skirmish::authorize(&state, &headers).await?;
    if !valid_id(&id) {
        return Err((StatusCode::NOT_FOUND, "unknown saved game".to_owned()));
    }
    let directory =
        directory(&state).ok_or((StatusCode::NOT_FOUND, "unknown saved game".to_owned()))?;
    for extension in ["json", "save"] {
        let _ = std::fs::remove_file(directory.join(format!("{id}.{extension}")));
    }
    Ok(StatusCode::NO_CONTENT)
}

pub(super) fn routes() -> axum::Router<AppState> {
    use axum::routing::{delete, get};
    axum::Router::new()
        .route("/saves", get(index).post(create))
        .route("/saves/{id}", delete(remove))
}

#[cfg(test)]
mod tests;

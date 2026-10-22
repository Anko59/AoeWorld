//! A live connection to one slot: joins with the slot token, keeps the world
//! model current from the server's fog-filtered messages, and sends orders.
use crate::{
    error::{Result, SdkError},
    events::Event,
    http::Endpoint,
    model::UnitKind,
    orders::Order,
    world::World,
};
use aoe_core::{TileCoord, TileRect};
use aoe_map::CompactChunk;
use aoe_protocol::{
    GAMEPLAY_VERSION, GameplayClientMessage, GameplayRole, GameplayServerMessage, ResumeToken,
    decode_gameplay_server, encode_gameplay_client,
};
use futures_util::{SinkExt, StreamExt};
use std::{
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::{
    sync::{Notify, mpsc},
    task::JoinHandle,
    time::{Instant, timeout},
};
use tokio_tungstenite::{connect_async, tungstenite::Message};

const MAX_BUFFERED_EVENTS: usize = 4_096;
const MAX_CHUNKS_IN_FLIGHT: usize = 4;
/// Orders per second the writer will not exceed (the server allows 100).
const ORDERS_PER_SECOND: usize = 90;
const RESUBSCRIBE_MIN_TICKS: u64 = 20;
const RESUBSCRIBE_DRIFT: i32 = 12;

/// Which part of the map the slot asks the server to describe. Own units and
/// buildings are always reported; the view bounds the *foreign* units, fog
/// tiles and wildlife the server computes and sends, so keep it as small as
/// the bot's decisions need.
#[derive(Clone, Copy, Debug)]
pub enum ViewPolicy {
    /// Track a window around the slot's own units and buildings.
    Follow { padding: i32, max_side: i32 },
    /// Always this region.
    Fixed(TileRect),
}

impl Default for ViewPolicy {
    fn default() -> Self {
        ViewPolicy::Follow {
            padding: 28,
            max_side: 128,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ConnectOptions {
    /// Decisions per second of game time (the server runs 20 ticks a second).
    pub decision_hz: f32,
    pub view: ViewPolicy,
    /// Read public map chunks so resources and walkable tiles are queryable.
    pub fetch_terrain: bool,
    pub connect_timeout: Duration,
}

impl Default for ConnectOptions {
    fn default() -> Self {
        Self {
            decision_hz: 2.0,
            view: ViewPolicy::default(),
            fetch_terrain: true,
            connect_timeout: Duration::from_secs(10),
        }
    }
}

/// What a bot receives at each decision point.
#[derive(Clone, Debug)]
pub struct Decision {
    pub world: World,
    /// Everything that happened since the previous decision.
    pub events: Vec<Event>,
}

struct State {
    world: World,
    events: Vec<Event>,
    closed: Option<String>,
}

struct Shared {
    state: Mutex<State>,
    notify: Notify,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }
}

enum Command {
    Order(u64, Order),
    View(ViewPolicy),
    Close,
}

pub struct Client {
    shared: Arc<Shared>,
    commands: mpsc::UnboundedSender<Command>,
    next_sequence: AtomicU64,
    decision_ticks: u64,
    last_decision: Mutex<Option<u64>>,
    task: Mutex<Option<JoinHandle<()>>>,
}

pub fn parse_token(hex: &str) -> Result<ResumeToken> {
    if hex.len() != 48 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(SdkError::InvalidToken);
    }
    let mut bytes = [0_u8; 24];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16)
            .map_err(|_| SdkError::InvalidToken)?;
    }
    Ok(ResumeToken(bytes))
}

impl Client {
    /// Joins the slot that `slot_token` (48 hex digits) belongs to.
    pub async fn connect(url: &str, slot_token: &str, options: ConnectOptions) -> Result<Client> {
        let token = parse_token(slot_token)?;
        let endpoint = Endpoint::parse(url)?;
        let (mut socket, _) = timeout(
            options.connect_timeout,
            connect_async(endpoint.websocket_url()),
        )
        .await
        .map_err(|_| SdkError::Timeout)?
        .map_err(|error| SdkError::WebSocket(error.to_string()))?;
        send_frame(
            &mut socket,
            &GameplayClientMessage::JoinSlot {
                version: GAMEPLAY_VERSION,
                token,
            },
        )
        .await?;
        let welcome = timeout(options.connect_timeout, next_message(&mut socket))
            .await
            .map_err(|_| SdkError::Timeout)??;
        if let GameplayServerMessage::Error { code, message } = &welcome {
            return Err(SdkError::Refused {
                code: *code,
                message: message.clone(),
            });
        }
        if !matches!(
            welcome,
            GameplayServerMessage::Welcome {
                role: GameplayRole::Controller,
                ..
            }
        ) {
            return Err(SdkError::Refused {
                code: 403,
                message: "the server did not seat this token as a controller".to_owned(),
            });
        }
        let mut world = World::from_welcome(&welcome)
            .ok_or_else(|| SdkError::Closed("no welcome".to_owned()))?;
        let tick_hz = world.tick_hz.max(1);
        let first = initial_region(&world);
        world.subscription = 1;
        send_frame(
            &mut socket,
            &GameplayClientMessage::Subscribe {
                revision: 1,
                region: first,
            },
        )
        .await?;
        let decision_ticks =
            ((tick_hz as f32 / options.decision_hz.max(0.05)).round() as u64).max(1);
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                world,
                events: Vec::new(),
                closed: None,
            }),
            notify: Notify::new(),
        });
        let (commands, receiver) = mpsc::unbounded_channel();
        let task = tokio::spawn(run(socket, shared.clone(), receiver, endpoint, options));
        Ok(Client {
            shared,
            commands,
            next_sequence: AtomicU64::new(1),
            decision_ticks,
            last_decision: Mutex::new(None),
            task: Mutex::new(Some(task)),
        })
    }

    /// Queues an order and returns its sequence number; the server's verdict
    /// arrives as `Event::OrderResult` with the same number.
    pub fn send(&self, order: Order) -> u64 {
        let sequence = self.next_sequence.fetch_add(1, Ordering::Relaxed);
        let _ = self.commands.send(Command::Order(sequence, order));
        sequence
    }

    pub fn train(&self, building: aoe_core::EntityId, kind: UnitKind) -> u64 {
        self.send(Order::Train { building, kind })
    }

    /// Changes how the slot's view region follows its units.
    pub fn set_view(&self, view: ViewPolicy) {
        let _ = self.commands.send(Command::View(view));
    }

    /// The current model without waiting for a decision point.
    pub fn world(&self) -> World {
        self.shared.lock().world.clone()
    }

    /// Waits for the next decision point: one every `1 / decision_hz` seconds
    /// of game time, once the first snapshot has arrived. Skipped decision
    /// points are not queued; a slow bot just sees later ticks.
    pub async fn next_decision(&self) -> Result<Decision> {
        loop {
            let notified = self.shared.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            {
                let mut state = self.shared.lock();
                if let Some(reason) = &state.closed {
                    return Err(SdkError::Closed(reason.clone()));
                }
                let mut last = self.last_decision.lock().unwrap_or_else(|e| e.into_inner());
                let due = last.is_none_or(|last| state.world.tick >= last + self.decision_ticks);
                if state.world.snapshot_seen && due {
                    *last = Some(state.world.tick);
                    return Ok(Decision {
                        world: state.world.clone(),
                        events: std::mem::take(&mut state.events),
                    });
                }
            }
            notified.await;
        }
    }

    pub fn is_closed(&self) -> bool {
        self.shared.lock().closed.is_some()
    }

    /// Why the connection ended, if it did.
    pub fn close_reason(&self) -> Option<String> {
        self.shared.lock().closed.clone()
    }

    /// Asks the connection to end; idempotent and usable from any thread.
    pub fn shutdown(&self) {
        let _ = self.commands.send(Command::Close);
    }

    /// Ends the connection and waits briefly for it to wind down.
    pub async fn close(self) {
        self.shutdown();
        let task = self.task.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(task) = task {
            let _ = timeout(Duration::from_secs(2), task).await;
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Close);
    }
}

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn send_frame(socket: &mut Socket, message: &GameplayClientMessage) -> Result<()> {
    let bytes = encode_gameplay_client(message)?;
    socket
        .send(Message::Binary(bytes.into()))
        .await
        .map_err(|error| SdkError::WebSocket(error.to_string()))
}

async fn next_message(socket: &mut Socket) -> Result<GameplayServerMessage> {
    loop {
        match socket.next().await {
            Some(Ok(Message::Binary(bytes))) => return Ok(decode_gameplay_server(&bytes)?),
            Some(Ok(Message::Close(_))) | None => {
                return Err(SdkError::Closed("server closed the connection".to_owned()));
            }
            Some(Ok(_)) => {}
            Some(Err(error)) => return Err(SdkError::WebSocket(error.to_string())),
        }
    }
}

/// Until the first snapshot tells where the slot's units are, look at a small
/// window at the map's centre; own units are reported regardless.
fn initial_region(world: &World) -> TileRect {
    let side = world.width.min(world.height).clamp(1, 64);
    TileRect::from_xywh(
        (world.width - side) / 2,
        (world.height - side) / 2,
        side,
        side,
    )
}

/// The window the policy wants for the model as it stands.
fn desired_region(world: &World, policy: ViewPolicy) -> Option<TileRect> {
    match policy {
        ViewPolicy::Fixed(region) => Some(region.clamp(world.width, world.height)),
        ViewPolicy::Follow { padding, max_side } => {
            let mut points = world
                .units
                .values()
                .filter(|u| Some(u.player) == world.me && u.kind != UnitKind::Scout)
                .map(|u| u.position.tile_floor())
                .chain(
                    world
                        .buildings
                        .values()
                        .filter(|b| Some(b.player) == world.me)
                        .map(|b| TileCoord::new(b.footprint.min.x, b.footprint.min.y)),
                )
                .peekable();
            points.peek()?;
            let (mut min_x, mut min_y, mut max_x, mut max_y) =
                (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
            for point in points {
                min_x = min_x.min(point.x);
                min_y = min_y.min(point.y);
                max_x = max_x.max(point.x);
                max_y = max_y.max(point.y);
            }
            let max_side = max_side.clamp(16, aoe_protocol::MAX_SUBSCRIPTION_TILES);
            let width = (max_x - min_x + 1 + 2 * padding).min(max_side);
            let height = (max_y - min_y + 1 + 2 * padding).min(max_side);
            let center = ((min_x + max_x) / 2, (min_y + max_y) / 2);
            Some(
                TileRect::from_xywh(center.0 - width / 2, center.1 - height / 2, width, height)
                    .clamp(world.width, world.height),
            )
        }
    }
}

fn drifted(a: TileRect, b: TileRect) -> bool {
    (a.min.x - b.min.x).abs() > RESUBSCRIBE_DRIFT
        || (a.min.y - b.min.y).abs() > RESUBSCRIBE_DRIFT
        || (a.width() - b.width()).abs() > RESUBSCRIBE_DRIFT
        || (a.height() - b.height()).abs() > RESUBSCRIBE_DRIFT
}

async fn run(
    mut socket: Socket,
    shared: Arc<Shared>,
    mut commands: mpsc::UnboundedReceiver<Command>,
    endpoint: Endpoint,
    options: ConnectOptions,
) {
    let (chunk_tx, mut chunk_rx) = mpsc::unbounded_channel::<((i32, i32), Result<CompactChunk>)>();
    let mut in_flight = 0_usize;
    let mut policy = options.view;
    let mut view = initial_region(&shared.lock().world);
    let mut view_tick = 0_u64;
    let (mut window_start, mut window_sent) = (Instant::now(), 0_usize);
    let reason = loop {
        tokio::select! {
            frame = socket.next() => {
                let message = match frame {
                    Some(Ok(Message::Binary(bytes))) => match decode_gameplay_server(&bytes) {
                        Ok(message) => message,
                        Err(error) => break format!("undecodable server message: {error}"),
                    },
                    Some(Ok(Message::Close(_))) | None => break "server closed the connection".to_owned(),
                    Some(Ok(_)) => continue,
                    Some(Err(error)) => break format!("websocket error: {error}"),
                };
                let reset = matches!(message, GameplayServerMessage::WorldReset { .. });
                let mut subscribe = None;
                let mut wanted = Vec::new();
                {
                    let mut state = shared.lock();
                    let events = state.world.apply(message);
                    state.events.extend(events);
                    let overflow = state.events.len().saturating_sub(MAX_BUFFERED_EVENTS);
                    state.events.drain(..overflow);
                    let world = &mut state.world;
                    if world.snapshot_seen
                        && world.tick >= view_tick + RESUBSCRIBE_MIN_TICKS
                        && let Some(want) = desired_region(world, policy)
                        && drifted(want, view)
                    {
                        view = want;
                        view_tick = world.tick;
                        world.subscription += 1;
                        subscribe = Some((world.subscription, want));
                    }
                    if world.fog_dirty && options.fetch_terrain {
                        world.fog_dirty = false;
                        if let Some(hash) = world.map_hash {
                            let chunks = world.fog.known_chunks();
                            let fresh = Arc::make_mut(&mut world.catalog)
                                .take_wanted(&chunks, MAX_CHUNKS_IN_FLIGHT.saturating_sub(in_flight));
                            wanted = fresh.into_iter().map(|chunk| (hash, chunk)).collect();
                        }
                    }
                }
                shared.notify.notify_waiters();
                for (hash, chunk) in wanted {
                    in_flight += 1;
                    let (endpoint, sender) = (endpoint.clone(), chunk_tx.clone());
                    tokio::spawn(async move {
                        let path = format!("/maps/{}/chunks/{}/{}", hex(&hash), chunk.0, chunk.1);
                        let _ = sender.send((chunk, endpoint.get_json::<CompactChunk>(&path).await));
                    });
                }
                if let Some((revision, region)) = subscribe
                    && let Err(error) = send_frame(&mut socket, &GameplayClientMessage::Subscribe { revision, region }).await
                {
                    break error.to_string();
                }
                if reset {
                    break "the server started a new world".to_owned();
                }
            }
            command = commands.recv() => match command {
                None | Some(Command::Close) => break "closed by the client".to_owned(),
                Some(Command::View(next)) => {
                    policy = next;
                    view_tick = 0;
                }
                Some(Command::Order(sequence, order)) => {
                    if window_start.elapsed() >= Duration::from_secs(1) {
                        (window_start, window_sent) = (Instant::now(), 0);
                    }
                    if window_sent >= ORDERS_PER_SECOND {
                        tokio::time::sleep_until(window_start + Duration::from_secs(1)).await;
                        (window_start, window_sent) = (Instant::now(), 0);
                    }
                    window_sent += 1;
                    let Some(message) = order.message(sequence) else {
                        shared.lock().events.push(Event::OrderResult {
                            sequence,
                            result: crate::model::CommandResult::RejectedInvalidDestination,
                        });
                        continue;
                    };
                    if let Err(error) = send_frame(&mut socket, &message).await {
                        break error.to_string();
                    }
                }
            },
            fetched = chunk_rx.recv() => {
                let Some((chunk, result)) = fetched else { continue };
                in_flight = in_flight.saturating_sub(1);
                let mut state = shared.lock();
                let world = &mut state.world;
                let catalog = Arc::make_mut(&mut world.catalog);
                match result.map_err(|error| error.to_string()).and_then(|data| catalog.insert(&data)) {
                    Ok(()) => {}
                    Err(_) => catalog.forget(chunk),
                }
                world.fog_dirty = true;
            }
        }
    };
    shared.lock().closed = Some(reason);
    shared.notify.notify_waiters();
}

fn hex(hash: &[u8; 32]) -> String {
    hash.iter().map(|byte| format!("{byte:02x}")).collect()
}

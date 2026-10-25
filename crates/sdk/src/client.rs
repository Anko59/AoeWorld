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
/// Orders per second the writer will not exceed by default (the server allows 100).
const ORDERS_PER_SECOND: usize = 90;
const RESUBSCRIBE_MIN_TICKS: u64 = 20;
const RESUBSCRIBE_DRIFT: i32 = 12;

/// Which part of the map the slot asks the server to describe. Own units and
/// buildings are always reported; the view bounds the *foreign* units, fog
/// tiles and wildlife the server computes and sends, so keep it as small as
/// the bot's decisions need.
#[derive(Clone, Copy, Debug)]
pub enum ViewPolicy {
    /// Track own units and buildings, centering on the living army when their
    /// spread exceeds the window. Own economy records are always reported.
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
    /// Orders per wall-clock second the writer allows (the arena lifts it).
    pub orders_per_second: usize,
}

impl Default for ConnectOptions {
    fn default() -> Self {
        Self {
            decision_hz: 2.0,
            view: ViewPolicy::default(),
            fetch_terrain: true,
            connect_timeout: Duration::from_secs(10),
            orders_per_second: ORDERS_PER_SECOND,
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
    drops: AtomicU64,
    rejoins: AtomicU64,
    reconnect_micros: AtomicU64,
}

/// How often the connection was lost and regained (the client rejoins its
/// slot by itself), so a healthy-looking client is not mistaken for one that
/// never dropped.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ConnectionStats {
    /// Connections lost without the client or a world reset ending them.
    pub drops: u64,
    /// Drops after which the slot was rejoined.
    pub rejoins: u64,
    /// Seconds spent between a drop and the rejoin (or the give-up).
    pub reconnect_seconds: f64,
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
        let (mut socket, welcome) = join(&endpoint, token, options.connect_timeout).await?;
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
            drops: AtomicU64::new(0),
            rejoins: AtomicU64::new(0),
            reconnect_micros: AtomicU64::new(0),
        });
        let (commands, receiver) = mpsc::unbounded_channel();
        let task = tokio::spawn(run(
            socket,
            shared.clone(),
            receiver,
            endpoint,
            options,
            token,
        ));
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

    /// How many orders this client has queued so far.
    pub fn orders_sent(&self) -> u64 {
        self.next_sequence.load(Ordering::Relaxed) - 1
    }

    pub fn train(&self, building: aoe_core::EntityId, kind: UnitKind) -> u64 {
        self.send(Order::Train { building, kind })
    }

    pub fn load(&self, units: &[aoe_core::EntityId], transport: aoe_core::EntityId) -> u64 {
        self.send(Order::Load {
            units: units.to_vec(),
            transport,
        })
    }

    pub fn unload(&self, transport: aoe_core::EntityId, to: TileCoord) -> u64 {
        self.send(Order::Unload { transport, to })
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

    /// Drops, rejoins and reconnect time so far.
    pub fn connection_stats(&self) -> ConnectionStats {
        ConnectionStats {
            drops: self.shared.drops.load(Ordering::Relaxed),
            rejoins: self.shared.rejoins.load(Ordering::Relaxed),
            reconnect_seconds: self.shared.reconnect_micros.load(Ordering::Relaxed) as f64 / 1e6,
        }
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

/// Opens a socket and presents the slot token; returns the controller's Welcome.
async fn join(
    endpoint: &Endpoint,
    token: ResumeToken,
    wait: Duration,
) -> Result<(Socket, GameplayServerMessage)> {
    let (mut socket, _) = timeout(wait, connect_async(endpoint.websocket_url()))
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
    let welcome = timeout(wait, next_message(&mut socket))
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
    Ok((socket, welcome))
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
                .filter(|u| Some(u.player) == world.me && u.hp > 0 && u.kind != UnitKind::Scout)
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
            let mut center = ((min_x + max_x) / 2, (min_y + max_y) / 2);
            if max_x - min_x + 1 + 2 * padding > max_side
                || max_y - min_y + 1 + 2 * padding > max_side
            {
                // Centering between a distant army and home describes empty
                // ground: even buildings beside our soldiers appear fogged.
                // Use the army's mean rather than its bounding box so fresh
                // reinforcements at home do not pull the view off the front.
                let (x, y, count) = world
                    .my_units(None)
                    .into_iter()
                    .filter(|u| u.is_military() && u.alive())
                    .fold((0_i64, 0_i64, 0_i64), |(x, y, n), u| {
                        let tile = u.tile();
                        (x + i64::from(tile.x), y + i64::from(tile.y), n + 1)
                    });
                if count > 0 {
                    center = ((x / count) as i32, (y / count) as i32);
                }
            }
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

mod session;
use session::run;

#[cfg(test)]
mod tests;

fn hex(hash: &[u8; 32]) -> String {
    hash.iter().map(|byte| format!("{byte:02x}")).collect()
}

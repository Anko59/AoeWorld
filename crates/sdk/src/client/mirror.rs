//! The in-process client state: what a socket session keeps (the world model,
//! the follow-view subscription, the wanted map chunks) without the socket.
//! The socket session and a seat played inside the server use this one code.
use crate::{client::ViewPolicy, events::Event, model::UnitKind, world::World};
use aoe_core::{TileCoord, TileRect};
use aoe_map::CompactChunk;
use aoe_protocol::{GameplayClientMessage, GameplayServerMessage};
use std::sync::Arc;

/// Ticks a window is held before the follow view may ask for another.
pub(crate) const RESUBSCRIBE_MIN_TICKS: u64 = 20;
const RESUBSCRIBE_DRIFT: i32 = 12;

/// A client's model of the match: fed the server's messages, it reports what
/// happened and what the client should ask for next.
pub struct Mirror {
    world: World,
    policy: ViewPolicy,
    view: TileRect,
    view_tick: u64,
}

impl Mirror {
    /// A mirror joined at subscription 1; `None` unless `welcome` is a `Welcome`.
    pub fn new(welcome: &GameplayServerMessage) -> Option<Mirror> {
        let mut world = World::from_welcome(welcome)?;
        world.subscription = 1;
        let view = initial_region(&world);
        Some(Mirror {
            world,
            policy: ViewPolicy::default(),
            view,
            view_tick: 0,
        })
    }

    /// The `Subscribe` that opens revision 1.
    pub fn first_subscribe(&self) -> GameplayClientMessage {
        GameplayClientMessage::Subscribe {
            revision: 1,
            region: self.view,
            unit_zoom: false,
        }
    }

    pub fn world(&self) -> &World {
        &self.world
    }

    pub fn policy(&self) -> ViewPolicy {
        self.policy
    }

    /// Changes the follow policy; the next frame may resubscribe at once.
    pub fn set_policy(&mut self, policy: ViewPolicy) {
        self.policy = policy;
        self.view_tick = 0;
    }

    /// Centers the view on `tile`, or restores `default` (a connection's own
    /// policy) when `None`.
    pub fn focus_view(&mut self, tile: Option<TileCoord>, default: ViewPolicy) {
        let size = self.world.map_size();
        self.set_policy(focus_policy(default, size, tile));
    }

    /// Folds one server message into the model and reports what happened.
    pub fn apply(&mut self, message: GameplayServerMessage) -> Vec<Event> {
        self.world.apply(message)
    }

    /// The next `Subscribe` the follow view wants, at most once per drift.
    pub fn take_subscribe(&mut self) -> Option<GameplayClientMessage> {
        let world = &mut self.world;
        if !world.snapshot_seen || world.tick < self.view_tick + RESUBSCRIBE_MIN_TICKS {
            return None;
        }
        let want = desired_region(world, self.policy)?;
        if !drifted(want, self.view) {
            return None;
        }
        self.view = want;
        self.view_tick = world.tick;
        world.subscription += 1;
        Some(GameplayClientMessage::Subscribe {
            revision: world.subscription,
            region: want,
            unit_zoom: false,
        })
    }

    /// Map hash and chunk of what the fog knows and was not yet requested,
    /// at most `limit`; empty until the fog changed.
    pub fn wanted_chunks(&mut self, limit: usize) -> Vec<([u8; 32], (i32, i32))> {
        let world = &mut self.world;
        if !world.fog_dirty {
            return Vec::new();
        }
        world.fog_dirty = false;
        let Some(hash) = world.map_hash else {
            return Vec::new();
        };
        let chunks = world.fog.known_chunks();
        Arc::make_mut(&mut world.catalog)
            .take_wanted(&chunks, limit)
            .into_iter()
            .map(|chunk| (hash, chunk))
            .collect()
    }

    /// A fetched chunk; a failed fetch lets it be requested again.
    pub fn insert_chunk(&mut self, chunk: (i32, i32), result: Result<CompactChunk, String>) {
        let catalog = Arc::make_mut(&mut self.world.catalog);
        if result.and_then(|data| catalog.insert(&data)).is_err() {
            catalog.forget(chunk);
        }
        self.world.fog_dirty = true;
    }
}

/// The policy that centers a view on `tile` (same size as `default`'s window),
/// or `default` itself without one.
pub(crate) fn focus_policy(
    default: ViewPolicy,
    (map_width, map_height): (i32, i32),
    tile: Option<TileCoord>,
) -> ViewPolicy {
    let Some(tile) = tile else {
        return default;
    };
    let (width, height) = match default {
        ViewPolicy::Follow { max_side, .. } => (max_side, max_side),
        ViewPolicy::Fixed(rect) => (rect.width(), rect.height()),
    };
    let bound = |span: i32, limit| {
        span.clamp(16, aoe_protocol::MAX_SUBSCRIPTION_TILES)
            .min(limit)
    };
    let (width, height) = (bound(width, map_width), bound(height, map_height));
    let x = tile.x.saturating_sub(width / 2).clamp(0, map_width - width);
    let y = tile
        .y
        .saturating_sub(height / 2)
        .clamp(0, map_height - height);
    ViewPolicy::Fixed(TileRect::from_xywh(x, y, width, height))
}

/// Until the first snapshot tells where the slot's units are, look at a small
/// window at the map's centre; own units are reported regardless.
pub(crate) fn initial_region(world: &World) -> TileRect {
    let side = world.width.min(world.height).clamp(1, 64);
    TileRect::from_xywh(
        (world.width - side) / 2,
        (world.height - side) / 2,
        side,
        side,
    )
}

/// The window the policy wants for the model as it stands.
pub(crate) fn desired_region(world: &World, policy: ViewPolicy) -> Option<TileRect> {
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

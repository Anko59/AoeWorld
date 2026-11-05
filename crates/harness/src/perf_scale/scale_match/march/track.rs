//! Per-member observation of one formation order, from what slot 0 sees.
use aoe_sdk::{EntityId, UnitKind, UnitTask, World};
use serde_json::{Value, json};

/// Ideal-time multiples at which arrivals are counted.
pub(super) const LIMITS: [f64; 3] = [1.3, 2.0, 3.0];
/// A member moving slower than this share of its nominal speed is stalled.
const STALL_SHARE: f64 = 0.2;
/// Distance samples per member are kept this often (wall seconds).
const TRACE_EVERY: f64 = 30.0;

/// Wall seconds and game tick of one observation.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Stamp {
    pub tick: u64,
    pub wall: f64,
}

#[derive(Debug)]
pub(super) struct Member {
    pub id: EntityId,
    pub kind: UnitKind,
    pub nominal: f64,
    start: (f64, f64),
    last: (f64, f64),
    last_tick: u64,
    pub first_motion: Option<Stamp>,
    pub arrived: Option<Stamp>,
    /// First time the member stood still or did something else short of the goal.
    pub dropped: Option<(Stamp, String, f64)>,
    pub gone: Option<Stamp>,
    pub travelled: f64,
    pub stall_ticks: u64,
    pub travel_ticks: u64,
    pub distance: f64,
    pub task: String,
    trace: Vec<(f64, f64)>,
}

fn tiles(world: &World, id: EntityId) -> Option<((f64, f64), UnitTask, bool)> {
    let unit = world.unit(id).filter(|u| u.alive())?;
    let p = unit.position();
    let tile = f64::from(aoe_core::FIXED_SUBUNITS_PER_TILE);
    Some((
        (f64::from(p.x) / tile, f64::from(p.y) / tile),
        unit.state.task,
        unit.state.moving || unit.state.planning,
    ))
}

impl Member {
    pub(super) fn new(world: &World, id: EntityId, nominal: f64) -> Option<Self> {
        let unit = world.unit(id)?;
        let (at, task, _) = tiles(world, id)?;
        Some(Self {
            id,
            kind: unit.kind(),
            nominal,
            start: at,
            last: at,
            last_tick: world.tick(),
            first_motion: None,
            arrived: None,
            dropped: None,
            gone: None,
            travelled: 0.0,
            stall_ticks: 0,
            travel_ticks: 0,
            distance: 0.0,
            task: format!("{task:?}"),
            trace: Vec::new(),
        })
    }

    /// Folds one observation in; `goal` is the order's tile centre and
    /// `tolerance` the slot radius in tiles.
    pub(super) fn observe(
        &mut self,
        world: &World,
        now: Stamp,
        order: Stamp,
        goal: (f64, f64),
        tolerance: f64,
    ) {
        if self.gone.is_some() {
            return;
        }
        let Some((at, task, moving)) = tiles(world, self.id) else {
            self.gone = Some(now);
            self.task = "Dead".to_owned();
            return;
        };
        let dt = now.tick.saturating_sub(self.last_tick);
        let step = (at.0 - self.last.0).hypot(at.1 - self.last.1);
        self.distance = (at.0 - goal.0).hypot(at.1 - goal.1);
        self.task = format!("{task:?}");
        if self.first_motion.is_none() && (at.0 - self.start.0).hypot(at.1 - self.start.1) > 0.5 {
            self.first_motion = Some(now);
        }
        if self.first_motion.is_some() && self.arrived.is_none() && dt > 0 {
            self.travelled += step;
            self.travel_ticks += dt;
            if step < STALL_SHARE * self.nominal * dt as f64 {
                self.stall_ticks += dt;
            }
        }
        if self.arrived.is_none() && self.distance <= tolerance {
            self.arrived = Some(now);
        }
        let still = !moving && task != UnitTask::Moving;
        let other = task != UnitTask::Moving && task != UnitTask::Idle;
        if self.arrived.is_none()
            && self.dropped.is_none()
            && (still || other)
            && now.tick > order.tick + 40
        {
            self.dropped = Some((now, format!("{task:?}"), self.distance));
        }
        if self
            .trace
            .last()
            .is_none_or(|(wall, _)| now.wall - wall >= TRACE_EVERY)
        {
            self.trace.push((now.wall - order.wall, self.distance));
        }
        self.last = at;
        self.last_tick = now.tick;
    }

    pub(super) fn speed_share(&self) -> Option<f64> {
        (self.travel_ticks > 0 && self.nominal > 0.0)
            .then(|| self.travelled / (self.travel_ticks as f64 * self.nominal))
    }

    pub(super) fn json(&self, order: Stamp) -> Value {
        let since = |s: Option<Stamp>| {
            s.map(|s| json!({"ticks": s.tick.saturating_sub(order.tick), "wall_s": s.wall - order.wall}))
        };
        json!({
            "id": self.id.0, "kind": format!("{:?}", self.kind),
            "first_motion": since(self.first_motion), "arrived": since(self.arrived),
            "dropped": self.dropped.as_ref().map(|(s, task, d)| json!({
                "ticks": s.tick.saturating_sub(order.tick), "wall_s": s.wall - order.wall,
                "task": task, "distance_tiles": d,
            })),
            "dead": since(self.gone),
            "travelled_tiles": self.travelled, "speed_share": self.speed_share(),
            "stall_ticks": self.stall_ticks, "travel_ticks": self.travel_ticks,
            "final_task": self.task, "final_distance_tiles": self.distance,
            "distance_trace": self.trace,
        })
    }
}

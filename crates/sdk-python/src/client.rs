//! `connect`, `start_match` and the `Client` a bot holds.
use crate::{
    enums::{BuildingKind, Tech, TradeResource, UnitKind},
    types::Event,
    world::World,
};
use aoe_sdk::{ConnectOptions, EntityId, Order, SdkError, TileCoord, ViewPolicy};
use pyo3::{exceptions::PyException, prelude::*};
use std::{
    sync::{Arc, OnceLock},
    time::Duration,
};
use tokio::runtime::Runtime;

pyo3::create_exception!(_native, AoeError, PyException, "Any SDK failure.");
pyo3::create_exception!(
    _native,
    ConnectionClosed,
    AoeError,
    "The connection to the server ended."
);
pyo3::create_exception!(
    _native,
    Refused,
    AoeError,
    "The server refused the slot token or the request."
);

/// The one network runtime of the process, started on first use.
pub(crate) fn runtime() -> PyResult<&'static Runtime> {
    static RUNTIME: OnceLock<Result<Runtime, String>> = OnceLock::new();
    RUNTIME
        .get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .thread_name("aoeworld-net")
                .build()
                .map_err(|error| error.to_string())
        })
        .as_ref()
        .map_err(|error| AoeError::new_err(format!("cannot start the network runtime: {error}")))
}

pub(crate) fn error(error: SdkError) -> PyErr {
    match error {
        SdkError::Closed(reason) => ConnectionClosed::new_err(reason),
        SdkError::Refused { .. } | SdkError::InvalidToken => Refused::new_err(error.to_string()),
        other => AoeError::new_err(other.to_string()),
    }
}

/// What one `wait_decision` call returns.
#[pyclass(frozen, get_all, module = "aoeworld")]
pub struct Decision {
    pub world: Py<World>,
    pub events: Vec<Event>,
    pub tick: u64,
}

#[pymethods]
impl Decision {
    fn __repr__(&self) -> String {
        format!("Decision(tick={}, events={})", self.tick, self.events.len())
    }
}

#[pyclass(frozen, get_all, module = "aoeworld")]
#[derive(Clone, Debug, Default)]
pub struct RunSummary {
    pub decisions: u64,
    pub last_tick: u64,
    pub closed: bool,
    pub close_reason: Option<String>,
}

/// A connection to one slot of a match.
#[pyclass(frozen, module = "aoeworld")]
pub struct Client {
    inner: Arc<aoe_sdk::Client>,
}

fn many(ids: Vec<u32>) -> Vec<EntityId> {
    ids.into_iter().map(EntityId).collect()
}

impl Client {
    fn order(&self, order: Order) -> u64 {
        self.inner.send(order)
    }

    pub(crate) fn decision(py: Python<'_>, decision: aoe_sdk::Decision) -> PyResult<Decision> {
        let tick = decision.world.tick();
        let events = decision.events.iter().map(Event::from).collect();
        Ok(Decision {
            world: Py::new(
                py,
                World {
                    inner: decision.world,
                },
            )?,
            events,
            tick,
        })
    }

    fn wait(&self, py: Python<'_>, timeout: Option<Duration>) -> PyResult<Option<Decision>> {
        let client = self.inner.clone();
        let rt = runtime()?;
        let waited = py.allow_threads(move || {
            rt.block_on(async move {
                match timeout {
                    Some(limit) => tokio::time::timeout(limit, client.next_decision())
                        .await
                        .ok(),
                    None => Some(client.next_decision().await),
                }
            })
        });
        match waited {
            None => Ok(None),
            Some(Ok(decision)) => Self::decision(py, decision).map(Some),
            Some(Err(failure)) => Err(error(failure)),
        }
    }
}

#[pymethods]
impl Client {
    /// Waits (GIL released) for the next decision point; `None` on timeout.
    #[pyo3(signature = (timeout=None))]
    fn wait_decision(&self, py: Python<'_>, timeout: Option<f64>) -> PyResult<Option<Decision>> {
        self.wait(
            py,
            timeout.map(|seconds| Duration::from_secs_f64(seconds.max(0.0))),
        )
    }

    /// The current world without waiting.
    #[getter]
    fn world(&self) -> World {
        World {
            inner: self.inner.world(),
        }
    }

    /// How many orders this client has queued so far.
    #[getter]
    fn orders_sent(&self) -> u64 {
        self.inner.orders_sent()
    }

    #[getter]
    fn closed(&self) -> bool {
        self.inner.is_closed()
    }

    /// `(drops, rejoins, reconnect_seconds)`: how often the connection was
    /// lost and regained by the client itself.
    #[getter]
    fn connection_stats(&self) -> (u64, u64, f64) {
        let stats = self.inner.connection_stats();
        (stats.drops, stats.rejoins, stats.reconnect_seconds)
    }

    #[getter]
    fn close_reason(&self) -> Option<String> {
        self.inner.close_reason()
    }

    /// Runs `on_tick(client, decision)` at every decision point until the
    /// game clock reaches `until_tick`, `max_decisions` decisions passed, the
    /// callback returns `False`, or the connection ends. The GIL is released
    /// while waiting, so other threads keep running; Ctrl-C works.
    #[pyo3(signature = (on_tick, until_tick=None, max_decisions=None))]
    fn run(
        slf: &Bound<'_, Self>,
        on_tick: PyObject,
        until_tick: Option<u64>,
        max_decisions: Option<u64>,
    ) -> PyResult<RunSummary> {
        let py = slf.py();
        let this = slf.get();
        let mut summary = RunSummary::default();
        loop {
            py.check_signals()?;
            let decision = match this.wait(py, Some(Duration::from_millis(500))) {
                Ok(Some(decision)) => decision,
                Ok(None) => continue,
                Err(failure) if failure.is_instance_of::<ConnectionClosed>(py) => {
                    summary.closed = true;
                    summary.close_reason = this.inner.close_reason();
                    return Ok(summary);
                }
                Err(failure) => return Err(failure),
            };
            summary.decisions += 1;
            summary.last_tick = decision.tick;
            let decision = Py::new(py, decision)?;
            let keep_going = on_tick.call1(py, (slf, decision))?;
            if !keep_going.is_none(py) && !keep_going.bind(py).is_truthy()? {
                return Ok(summary);
            }
            if until_tick.is_some_and(|tick| summary.last_tick >= tick)
                || max_decisions.is_some_and(|count| summary.decisions >= count)
            {
                return Ok(summary);
            }
        }
    }

    /// Ends the connection (idempotent).
    fn close(&self) {
        self.inner.shutdown();
    }

    /// Walk a unit to tile `(x, y)`. Every order returns its sequence number;
    /// the server's verdict arrives as an `order_result` event.
    #[pyo3(name = "move")]
    fn move_unit(&self, unit: u32, x: i32, y: i32) -> u64 {
        self.order(Order::Move {
            unit: EntityId(unit),
            to: TileCoord::new(x, y),
        })
    }
    fn queue_move(&self, unit: u32, x: i32, y: i32) -> u64 {
        self.order(Order::QueueMove { unit:EntityId(unit),to:TileCoord::new(x,y) })
    }
    fn stop(&self, unit: u32) -> u64 {
        self.order(Order::Stop {
            unit: EntityId(unit),
        })
    }
    fn set_rally(&self, building: u32, x: i32, y: i32) -> PyResult<u64> {
        let p = aoe_sdk::WorldPosition::from_tile_center(TileCoord::new(x, y))
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        Ok(self.order(Order::SetRally {
            building: EntityId(building),
            target: aoe_sdk::RallyTarget::Ground(p),
        }))
    }
    fn set_rally_resource(&self, building: u32, resource: u64) -> u64 {
        self.order(Order::SetRally {
            building: EntityId(building),
            target: aoe_sdk::RallyTarget::Resource(resource),
        })
    }
    fn set_rally_building(&self, building: u32, target: u32) -> u64 {
        self.order(Order::SetRally {
            building: EntityId(building),
            target: aoe_sdk::RallyTarget::Building(EntityId(target)),
        })
    }
    fn set_rally_animal(&self, building: u32, animal: u32) -> u64 {
        self.order(Order::SetRally {
            building: EntityId(building),
            target: aoe_sdk::RallyTarget::Animal(EntityId(animal)),
        })
    }
    fn gather(&self, unit: u32, resource: u64) -> u64 {
        self.order(Order::Gather {
            unit: EntityId(unit),
            resource,
        })
    }
    fn build(&self, builders: Vec<u32>, kind: BuildingKind, x: i32, y: i32) -> u64 {
        self.order(Order::Build {
            builders: many(builders),
            kind: kind.into(),
            origin: TileCoord::new(x, y),
        })
    }
    fn build_wall(&self, builders: Vec<u32>, x1: i32, y1: i32, x2: i32, y2: i32) -> u64 {
        self.order(Order::BuildWall {
            builders: many(builders),
            from: TileCoord::new(x1, y1),
            to: TileCoord::new(x2, y2),
        })
    }
    fn construct(&self, builders: Vec<u32>, building: u32) -> u64 {
        self.order(Order::Construct {
            builders: many(builders),
            building: EntityId(building),
        })
    }
    fn cancel_construction(&self, building: u32) -> u64 {
        self.order(Order::CancelConstruction {
            building: EntityId(building),
        })
    }
    #[pyo3(signature = (unit, enabled=true))]
    fn auto_scout(&self, unit: u32, enabled: bool) -> u64 {
        self.order(Order::AutoScout { unit: EntityId(unit), enabled })
    }
    fn farm(&self, unit: u32, farm: u32) -> u64 {
        self.order(Order::Farm {
            unit: EntityId(unit),
            farm: EntityId(farm),
        })
    }
    /// Prepay or cancel one farm rebuild at a Mill.
    #[pyo3(signature = (building, add=true))]
    fn farm_reseed(&self, building: u32, add: bool) -> u64 {
        self.order(Order::FarmReseed { building: EntityId(building), add })
    }
    fn train(&self, building: u32, kind: UnitKind) -> u64 {
        self.order(Order::Train {
            building: EntityId(building),
            kind: kind.into(),
        })
    }
    fn cancel_training(&self, building: u32) -> u64 {
        self.order(Order::CancelTraining {
            building: EntityId(building),
        })
    }
    fn research(&self, building: u32, tech: Tech) -> u64 {
        self.order(Order::Research {
            building: EntityId(building),
            tech: tech.into(),
        })
    }
    fn cancel_research(&self, building: u32) -> u64 {
        self.order(Order::CancelResearch {
            building: EntityId(building),
        })
    }
    fn attack(&self, unit: u32, target: u32) -> u64 {
        self.order(Order::Attack {
            unit: EntityId(unit),
            target: EntityId(target),
        })
    }
    fn hunt(&self, units: Vec<u32>, animal: u32) -> u64 {
        self.order(Order::Hunt {
            units: many(units),
            animal: EntityId(animal),
        })
    }
    fn market_buy(&self, building: u32, resource: TradeResource) -> u64 {
        self.order(Order::MarketBuy {
            building: EntityId(building),
            resource: resource.into(),
        })
    }
    fn market_sell(&self, building: u32, resource: TradeResource) -> u64 {
        self.order(Order::MarketSell {
            building: EntityId(building),
            resource: resource.into(),
        })
    }

    fn __enter__(slf: Py<Self>) -> Py<Self> {
        slf
    }
    #[pyo3(signature = (*_args))]
    fn __exit__(&self, _args: &Bound<'_, pyo3::types::PyTuple>) {
        self.inner.shutdown();
    }
}

mod fleet;
pub use fleet::Fleet;

/// Joins the slot `slot_token` belongs to. `decision_hz` is how many decision
/// points per game second the bot gets (the game runs 20 ticks a second);
/// `view_padding` and `view_size` bound the map window the server describes.
#[pyfunction]
#[pyo3(signature = (url, slot_token, decision_hz=2.0, view_padding=28, view_size=128, fetch_terrain=true, orders_per_second=90))]
pub fn connect(
    py: Python<'_>,
    url: String,
    slot_token: String,
    decision_hz: f32,
    view_padding: i32,
    view_size: i32,
    fetch_terrain: bool,
    orders_per_second: usize,
) -> PyResult<Client> {
    let options = ConnectOptions {
        decision_hz,
        view: ViewPolicy::Follow {
            padding: view_padding,
            max_side: view_size,
        },
        fetch_terrain,
        orders_per_second: orders_per_second.max(1),
        ..ConnectOptions::default()
    };
    let rt = runtime()?;
    let client = py
        .allow_threads(move || rt.block_on(aoe_sdk::Client::connect(&url, &slot_token, options)))
        .map_err(error)?;
    Ok(Client {
        inner: Arc::new(client),
    })
}

//! The whole application over HTTP, for the tests of its secrets.
use crate::gameplay::SlotSetup;
use crate::{AppState, Config, GameplayService, app};
use aoe_core::{PlayerId, Seed, TileCoord, WorldConfig};
use aoe_scenario::SMOKE;
use aoe_simulation::GameWorld;
use std::{net::SocketAddr, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::RwLock,
    task::JoinHandle,
    time::timeout,
};

mod secret;

const SECRET: &str = "authority-test-secret-0123456789";
/// A deadline for progress, not a speed bound.
const DEADLINE: Duration = Duration::from_secs(60);

struct Server {
    address: SocketAddr,
    task: JoinHandle<()>,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// The whole application over HTTP, serving `gameplay`, without a clock.
async fn serve(gameplay: GameplayService) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let config = Config {
        bind: address,
        scenario: SMOKE,
        tick_hz: 20,
        asset_pack: None,
        map_package_directory: None,
        open_map_package: None,
        map_worker: None,
        geodata_cache_directory: ".cache/geodata".into(),
    };
    let mut state = AppState::new(&config, "authority-test").unwrap();
    state.gameplay = Arc::new(RwLock::new(gameplay));
    state.match_admin_token = Some(SECRET.into());
    let router = app(state);
    let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    Server { address, task }
}

/// One HTTP request, with the credential it presents if any.
#[derive(Clone)]
struct Ask {
    method: &'static str,
    path: String,
    header: Option<(&'static str, String)>,
    body: String,
}

impl Ask {
    fn new(method: &'static str, path: impl Into<String>) -> Self {
        Self {
            method,
            path: path.into(),
            header: None,
            body: String::new(),
        }
    }

    fn with_secret(mut self, secret: &str) -> Self {
        self.header = Some(("X-AoeWorld-Admin-Token", secret.to_owned()));
        self
    }

    fn json(mut self, body: &impl serde::Serialize) -> Self {
        self.body = serde_json::to_string(body).unwrap();
        self
    }

    async fn send(self, address: SocketAddr) -> (u16, String) {
        let header = self
            .header
            .map(|(name, value)| format!("{name}: {value}\r\n"))
            .unwrap_or_default();
        let request = format!(
            "{} {} HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\n{header}Content-Length: {}\r\nConnection: close\r\n\r\n{}",
            self.method,
            self.path,
            self.body.len(),
            self.body
        );
        let exchange = async {
            let mut stream = TcpStream::connect(address).await.unwrap();
            stream.write_all(request.as_bytes()).await.unwrap();
            let mut text = String::new();
            stream.read_to_string(&mut text).await.unwrap();
            let status = text.split_whitespace().nth(1).unwrap().parse().unwrap();
            (status, text.split_once("\r\n\r\n").unwrap().1.to_owned())
        };
        timeout(DEADLINE, exchange).await.expect("an answer")
    }
}

/// A small world with a village for each of `people`.
fn villages(people: usize) -> GameplayService {
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap();
    let setups = [(0, 32), (1, 180)][..people]
        .iter()
        .map(|&(player, at)| {
            let player = PlayerId(player);
            let start = TileCoord::new(at, at);
            SlotSetup {
                player,
                primary_unit_id: world.initialize_village(player, start).unwrap(),
                human: true,
            }
        })
        .collect();
    GameplayService::from_world_slots(world, setups, Some([7; 32]), None)
}

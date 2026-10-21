//! The SDK against a real local server: two bots in one match.
use aoe_scenario::SMOKE;
use aoe_sdk::{
    Bot, BuildingKind, Client, ConnectOptions, Decision, MatchMap, Order, ResourceKind, RunLimits,
    SlotKind, TileCoord, UnitKind, run_bot, start_match,
};
use aoe_server::{AppState, Config, app};
use std::net::SocketAddr;
use tokio::net::TcpListener;

const SECRET: &str = "sdk-test-secret-0123456789";

async fn server() -> SocketAddr {
    // The flat test ground is stored as a map package: give it a directory.
    let packages = tempfile::tempdir().unwrap().keep();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let state = AppState::new(
        &Config {
            bind: address,
            scenario: SMOKE,
            tick_hz: 60,
            asset_pack: None,
            map_package_directory: Some(packages),
            open_map_package: None,
            map_worker: None,
            geodata_cache_directory: ".cache/geodata".into(),
        },
        "sdk-test",
    )
    .unwrap()
    .with_match_admin_token(SECRET);
    tokio::spawn(state.clone().run_ticks());
    tokio::spawn(async move { axum::serve(listener, app(state)).await.unwrap() });
    address
}

/// The scout circles the base in widening rings to find trees; villagers
/// then fetch wood and the Town Center keeps training villagers.
#[derive(Default)]
struct EconomyBot {
    waypoints: Vec<TileCoord>,
    next: usize,
}

impl EconomyBot {
    fn plan(&mut self, around: TileCoord) {
        for radius in [10.0_f64, 20.0, 30.0, 40.0] {
            for step in 0..10 {
                let angle = std::f64::consts::TAU * f64::from(step) / 10.0;
                self.waypoints.push(TileCoord::new(
                    around.x + (angle.cos() * radius) as i32,
                    around.y + (angle.sin() * radius) as i32,
                ));
            }
        }
    }
}

impl Bot for EconomyBot {
    fn decide(&mut self, decision: &Decision, orders: &mut Vec<Order>) {
        let world = &decision.world;
        let (Some(center), Some(economy)) = (world.town_center(), world.economy()) else {
            return;
        };
        if self.waypoints.is_empty() {
            self.plan(center.center());
        }
        for scout in world.idle_units(Some(UnitKind::Scout)) {
            if let Some(to) = self.waypoints.get(self.next).copied() {
                self.next += 1;
                orders.push(Order::Move {
                    unit: scout.id(),
                    to,
                });
            }
        }
        for villager in world.idle_units(Some(UnitKind::Villager)) {
            if let Some(node) = world.nearest_resource(ResourceKind::Wood, villager.tile()) {
                orders.push(Order::Gather {
                    unit: villager.id(),
                    resource: node.id,
                });
            }
        }
        if center.state.training_queue.is_empty()
            && economy.stockpile().food >= 50
            && economy.free_population() > 0
        {
            orders.push(Order::Train {
                building: center.id(),
                kind: UnitKind::Villager,
            });
        }
    }
}

#[tokio::test]
async fn two_bots_join_their_slots_and_grow() {
    let address = server().await;
    let url = format!("http://{address}");
    let info = start_match(
        &url,
        SECRET,
        &MatchMap::flat(500, 3),
        &[SlotKind::Ai, SlotKind::Ai],
    )
    .await
    .unwrap();
    assert_eq!(info.slots.len(), 2);
    let options = ConnectOptions {
        decision_hz: 2.0,
        ..ConnectOptions::default()
    };
    let mut clients = Vec::new();
    for slot in &info.slots {
        clients.push(
            Client::connect(&url, &slot.token, options.clone())
                .await
                .unwrap(),
        );
    }
    let run = |client: Client| async move {
        let mut bot = EconomyBot::default();
        let limits = RunLimits {
            until_tick: Some(1_500),
            max_decisions: None,
        };
        let (summary, result) = run_bot(&client, &mut bot, limits).await;
        result.unwrap();
        (summary, client.world())
    };
    let second = clients.pop().unwrap();
    let first = clients.pop().unwrap();
    let ((a_summary, a_world), (b_summary, b_world)) = tokio::join!(run(first), run(second));
    for (summary, world, slot) in [
        (&a_summary, &a_world, &info.slots[0]),
        (&b_summary, &b_world, &info.slots[1]),
    ] {
        assert_eq!(world.me().map(|p| p.0), Some(slot.player));
        assert!(summary.orders_sent > 3, "{summary:?}");
        assert!(summary.orders_accepted > 0, "{summary:?}");
        let villagers = world.my_units(Some(UnitKind::Villager)).len();
        assert!(villagers > 3, "the economy grew: {villagers} villagers");
        assert!(world.loaded_chunks() > 0, "map chunks were read");
        let stock = world.economy().unwrap().stockpile();
        assert!(stock.wood > 0 || stock.food > 0);
        assert!(
            !world
                .my_buildings(Some(BuildingKind::TownCenter))
                .is_empty()
        );
        // Fog: the opponent is far away and never shows up.
        assert!(world.visible_enemies().is_empty());
        assert!(world.known_enemy_buildings().is_empty());
    }
}

#[tokio::test]
async fn a_wrong_token_is_refused_and_a_bad_one_is_rejected_locally() {
    let address = server().await;
    let url = format!("http://{address}");
    let info = start_match(&url, SECRET, &MatchMap::flat(500, 3), &[SlotKind::Ai])
        .await
        .unwrap();
    let mut wrong = info.slots[0].token.clone();
    wrong.replace_range(0..2, if &wrong[0..2] == "00" { "01" } else { "00" });
    let refused = Client::connect(&url, &wrong, ConnectOptions::default()).await;
    assert!(matches!(
        refused,
        Err(aoe_sdk::SdkError::Refused { code: 403, .. })
    ));
    assert!(matches!(
        Client::connect(&url, "xyz", ConnectOptions::default()).await,
        Err(aoe_sdk::SdkError::InvalidToken)
    ));
    assert!(
        start_match(
            &url,
            "wrong-secret",
            &MatchMap::flat(500, 3),
            &[SlotKind::Ai]
        )
        .await
        .is_err()
    );
}

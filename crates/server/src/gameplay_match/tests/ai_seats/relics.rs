use super::*;

#[tokio::test]
async fn a_castle_age_rust_ai_builds_a_monastery_and_banks_a_relic_in_skirmish() {
    use crate::gameplay_match::{flat, launch};
    use aoe_simulation::{Age, GameWorld, Tech};

    let (package, pages) = flat::package(flat::FlatMap {
        tiles_per_side: 384,
        seed: 7,
    })
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    crate::map_store::persist_prepared(Some(dir.path()), &package, &pages, &[], &[], &[]).unwrap();
    let config = crate::Config {
        bind: "127.0.0.1:0".parse().unwrap(),
        scenario: aoe_scenario::SMOKE,
        tick_hz: 20,
        asset_pack: None,
        map_package_directory: Some(dir.path().to_owned()),
        open_map_package: None,
        map_worker: None,
        geodata_cache_directory: dir.path().to_owned(),
    };
    let state = AppState::new(&config, "ai-relic-skirmish").unwrap();
    let game = launch(
        &state,
        package,
        vec![true, false],
        MatchRules::default().with_seed(7),
    )
    .await
    .unwrap()
    .service;
    // Run the ordinary decision and order path on a shorter test clock.
    const HZ: u32 = 4;
    game.world.write().await.set_tick_hz(HZ);
    *state.gameplay.write().await = game.clone();
    assert_eq!(
        crate::gameplay_match::play_ai_seats(&state, AiRuntime::Rust).await,
        1
    );
    let ai = aoe_core::PlayerId(1);
    let human = aoe_core::PlayerId(0);
    let tribute_site = game
        .world
        .read()
        .await
        .buildings()
        .find(|b| b.kind == BuildingKind::TownCenter && b.player == human)
        .unwrap()
        .footprint
        .min;
    let monk_gold = GameWorld::unit_training(UnitKind::Monk).unwrap().cost.gold;
    let collection_gold = Tech::ImperialAge.rules().cost.gold + monk_gold;
    let mut exposed_relic = false;
    let mut built_monastery = false;
    let mut funded_monk = false;
    for _ in 0..45 * 60 * HZ {
        game.tick().await;
        let mut world = game.world.write().await;
        if world.player_economy(ai).unwrap().age < Age::Castle {
            continue;
        }
        if !exposed_relic {
            let center = world
                .buildings()
                .find(|b| b.kind == BuildingKind::TownCenter && b.player == ai)
                .unwrap()
                .footprint
                .min;
            let distance = |at: aoe_core::WorldPosition| {
                let tile = at.tile_floor();
                i64::from(tile.x - center.x).pow(2) + i64::from(tile.y - center.y).pow(2)
            };
            let (_, at) = world
                .ground_relics()
                .min_by_key(|&(_, at)| distance(at))
                .unwrap();
            assert!(
                distance(at) <= 45_i64.pow(2),
                "the fixture needs a nearby relic"
            );
            // Expose a naturally seeded relic once the ordinary AI reaches Castle.
            // An idle cart keeps sight of it without occupying its pickup tile.
            let tile = at.tile_floor();
            let watch = [(-3, 0), (3, 0), (0, -3), (0, 3)]
                .into_iter()
                .map(|(dx, dy)| TileCoord::new(tile.x + dx, tile.y + dy))
                .find(|&tile| world.tile_passable(tile))
                .unwrap();
            world
                .spawn_kind(
                    ai,
                    UnitKind::TradeCart,
                    aoe_core::WorldPosition::from_tile_center(watch).unwrap(),
                )
                .unwrap();
            assert_eq!(
                world.visibility(ai, at.tile_floor()),
                aoe_simulation::TileVisibility::Visible
            );
            exposed_relic = true;
        }
        for monastery in world
            .buildings()
            .filter(|b| b.kind == BuildingKind::Monastery && b.player == ai && b.is_complete())
        {
            built_monastery = true;
            if world.monastery_relic_count(monastery.id) > 0 {
                return;
            }
        }
        // Fund one collector beyond the AI's Imperial Age reserve. The test
        // still relies on AI orders for Castle Age, construction and collection.
        let gold = world.player_economy(ai).unwrap().stockpile.gold;
        if built_monastery && !funded_monk && gold < collection_gold {
            fund_relic_skirmish(&mut world, tribute_site, human, ai, collection_gold - gold);
            funded_monk = true;
        }
    }
    assert!(exposed_relic, "the Rust AI did not reach Castle Age");
    assert!(built_monastery, "the Rust AI never finished a Monastery");
    let world = game.world.read().await;
    let orders = game
        .ai_order_log()
        .into_iter()
        .rev()
        .filter(|order| {
            order.player == ai
                && ["Monastery", "Monk", "Relic", "Garrison"]
                    .iter()
                    .any(|kind| order.order.contains(kind))
        })
        .take(8)
        .collect::<Vec<_>>();
    let monks = world
        .units()
        .filter(|unit| unit.player == ai && unit.kind == UnitKind::Monk)
        .map(|monk| {
            (
                monk.position.tile_floor(),
                monk.task,
                world.carried_relic(monk.id),
            )
        })
        .collect::<Vec<_>>();
    panic!(
        "the Rust AI never banked a relic. Monks: {monks:?}, recent orders: {orders:?}, stockpile: {:?}",
        world.player_economy(ai).unwrap().stockpile,
    );
}

fn fund_relic_skirmish(
    world: &mut aoe_simulation::GameWorld,
    center: TileCoord,
    human: aoe_core::PlayerId,
    ai: aoe_core::PlayerId,
    gold: u32,
) {
    use aoe_simulation::LateGameSeat;

    world
        .seed_late_game(
            human,
            center,
            &LateGameSeat {
                buildings: vec![(BuildingKind::Market, 1)],
                radius: 16,
                stockpile: 2 * gold,
                ..LateGameSeat::default()
            },
        )
        .unwrap();
    let market = world.tribute_market(human).unwrap();
    world
        .issue_tribute(human, ai, market, aoe_map::ResourceKind::Gold, gold)
        .unwrap();
}

use super::*;
use aoe_simulation::OrderIssuer;

#[tokio::test]
async fn route_demand_reports_movers_and_supersessions_inside_ten_seconds() {
    for elapsed in [0, 199, 200] {
        let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap();
        let id = world.spawn_unit(PlayerId(0), at(20, 20)).unwrap();
        world.spawn_unit(PlayerId(0), at(20, 30)).unwrap();
        world
            .with_order_issuer(OrderIssuer::PythonAi, |w| w.issue_move(id, at(200, 20)))
            .unwrap();
        for _ in 0..elapsed {
            world.advance();
        }
        assert!(world.unit(id).unwrap().moving);
        let gameplay = GameplayService::from_scale_world_slots(world, [(PlayerId(0), id, true)]);
        let server = server(gameplay, true).await;
        let (status, body) = request(&server, "GET", "/admin/match/routes", Some(SECRET), "").await;
        assert_eq!(status, 200);
        assert_eq!(body["units"], 2);
        assert_eq!(body["movers"], 1);
        let service = server.state.gameplay.read().await.clone();
        service
            .world
            .write()
            .await
            .issue_stop(PlayerId(0), id)
            .unwrap();
        let (status, body) = request(&server, "GET", "/admin/match/routes", Some(SECRET), "").await;
        assert_eq!(status, 200);
        assert_eq!(body["movers"], 0);
        let row = &body["order_demand"]["issuers"]["python_ai"];
        assert_eq!(row["superseded"], 1);
        assert_eq!(row["superseded_10s"], u64::from(elapsed < 200));
        let sources = body["order_demand"]["sources"]
            .as_object()
            .expect("caller breakdown is an object even when empty");
        assert!(
            sources
                .keys()
                .all(|site| site.starts_with("python_ai -> rally_automation | "))
        );
        assert_eq!(
            sources
                .values()
                .map(|v| v["superseded_10s"].as_u64().unwrap())
                .sum::<u64>(),
            u64::from(elapsed < 200)
        );
    }
}

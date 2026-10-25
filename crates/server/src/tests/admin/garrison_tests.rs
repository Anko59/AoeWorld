use super::*;
use aoe_simulation::{BuildingKind, UnitKind};

#[tokio::test]
async fn army_centroid_uses_town_center_shelters_and_weights_field_units() {
    let kind = BuildingKind::TownCenter;
    for mixed in [false, true] {
        let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap();
        let side = kind.rules().side;
        let shelter = world
            .place_town_center(PlayerId(0), TileCoord::new(100, 100))
            .unwrap();
        let soldier = world
            .spawn_kind(PlayerId(0), UnitKind::Militia, at(99, 102))
            .unwrap();
        if mixed {
            world
                .spawn_kind(PlayerId(0), UnitKind::Militia, at(120, 110))
                .unwrap();
        }
        world.issue_garrison(PlayerId(0), soldier, shelter).unwrap();
        for _ in 0..100 {
            world.advance();
            if world.unit(soldier).is_none() {
                break;
            }
        }
        assert!(world.unit(soldier).is_none());
        assert_eq!(world.building(shelter).unwrap().garrison.len(), 1);
        let gameplay =
            GameplayService::from_scale_world_slots(world, [(PlayerId(0), soldier, true)]);
        let server = server(gameplay, true).await;
        let (status, body) = request(&server, "GET", "/admin/match", Some(SECRET), "").await;
        assert_eq!(status, 200);
        let player = &body["players"][0];
        let center = 100.0 + f64::from(side) / 2.0;
        let expected = if mixed {
            json!({"x": (center + 120.5) / 2.0, "y": (center + 110.5) / 2.0})
        } else {
            json!({"x": center, "y": center})
        };
        assert_eq!(
            player["army"]["centroid"], expected,
            "{kind:?}, mixed={mixed}"
        );
        assert_eq!(player["army"]["size"], if mixed { 2 } else { 1 });
        assert_eq!(player["population"], if mixed { 2 } else { 1 });
    }
}

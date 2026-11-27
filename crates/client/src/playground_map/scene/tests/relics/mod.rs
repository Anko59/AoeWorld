use super::review_tests::{client_with_backend, contact_scene};
use crate::playground::game::village::relics;
use crate::unit_visuals::Pose;
use aoe_core::EntityId;
use aoe_protocol::{GroundRelic, RelicView, UnitKind};
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
async fn visible_relics_and_monk_carry_art_follow_the_subscription() {
    let scene = contact_scene();
    let mut client = client_with_backend(&scene, Some("canvas")).await;
    let id = EntityId(1);
    let monk = client.units.get_mut(&id).unwrap();
    monk.kind = UnitKind::Monk;
    let ground = GroundRelic {
        id: EntityId(20),
        position: monk.position,
    };
    client.revision = 7;
    relics::sync(
        &mut client,
        RelicView {
            revision: 7,
            ground: vec![ground],
            carriers: vec![id],
            ..RelicView::default()
        },
    );
    assert_eq!(relics::ground(&client), [ground]);
    assert_eq!(
        super::actors::shown_pose(&client, &client.units[&id]).1,
        Pose::MonkWithRelicStand
    );
    client.revision += 1;
    assert!(relics::ground(&client).is_empty());
    assert!(!relics::carries(&client, id));
    client.canvas.remove();
}

#[wasm_bindgen_test]
async fn placement_preview_rejects_a_relic_on_the_interior_tile() {
    use crate::playground::game::village::building_panel;
    use aoe_core::{PlayerId, ScreenPoint, Tick, TileCoord, TileRect, WorldPosition};
    use aoe_protocol::{Age, BuildingKind, FogState, GameplayPlayerState, Stockpile};

    let scene = contact_scene();
    let mut client = client_with_backend(&scene, Some("canvas")).await;
    let id = EntityId(1);
    let builder = client.units.get_mut(&id).unwrap();
    builder.kind = UnitKind::Villager;
    builder.player = PlayerId(0);
    client.selected.units(vec![id]);
    client.village.players.push(GameplayPlayerState {
        player: PlayerId(0),
        age: Age::Dark,
        researched: 0,
        stockpile: Stockpile {
            wood: 175,
            ..Stockpile::default()
        },
        population_used: 1,
        population_cap: 5,
        recent_deliveries: [0; 4],
        unit_counts: [0; UnitKind::ALL.len()],
        held_tiles: 0,
        held_share_ppm: 0,
        market_prices: Default::default(),
    });
    client.fog.apply(FogState {
        revision: 1,
        tick: Tick(0),
        region: TileRect::from_xywh(0, 0, 1, 1),
        tiles: vec![],
        overview: vec![],
        disabled: true,
    });
    client.village.placement = Some(BuildingKind::Barracks);
    client.pointer = Some(ScreenPoint { x: 256.0, y: 256.0 });

    let mut ghosts = Vec::new();
    building_panel::preview(&client, &mut ghosts);
    assert_eq!(ghosts.len(), 1);
    let clear = ghosts.pop().unwrap();
    assert_eq!(clear.visual, clear.visual.with_ghost(true));
    let interior = TileCoord::new(
        clear.position[0].floor() as i32,
        clear.position[1].floor() as i32,
    );
    let revision = client.revision;
    relics::sync(
        &mut client,
        RelicView {
            revision,
            ground: vec![GroundRelic {
                id: EntityId(20),
                position: WorldPosition::from_tile_center(interior).unwrap(),
            }],
            ..RelicView::default()
        },
    );
    building_panel::preview(&client, &mut ghosts);
    assert_eq!(ghosts.len(), 1);
    assert_eq!(ghosts[0].visual, clear.visual.with_ghost(false));
    client.canvas.remove();
}

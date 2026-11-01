use super::*;
use aoe_core::{ScreenPoint, TileCoord, TileRect};
use aoe_protocol::{Age, BuildingKind, ConstructionState, GameplayBuildingState};
use aoe_rendering::{AtlasAddress, GameFrame};

#[wasm_bindgen_test]
async fn zoomed_gate_culling_keeps_overlapping_sprite_and_skips_offscreen_sprite() {
    let mut camera = Camera {
        center: [100.0, 100.5],
        zoom: 3.0,
        viewport: [1280.0, 720.0],
        focus_elevation_meters: 0.0,
    };
    let offset = camera.screen_to_world(ScreenPoint {
        x: 1540.0,
        y: 360.0,
    });
    camera.center[0] -= offset[0] - 100.0;
    camera.center[1] -= offset[1] - 100.5;
    let scene = PreparedScene::new(camera, Vec::new());
    let mut client = client(&scene).await;
    client.units.clear();
    client.canvas.set_width(1280);
    client.canvas.set_height(720);
    // Native frame at zoom 3 projects to [1148, -168, 1820, 488].
    let frame = GameFrame {
        stream: 0,
        atlas: AtlasAddress::WHITE,
        size: [168.0, 164.0],
        anchor: [98.0, 132.0],
    };
    for slot in [31, 42, 43] {
        client.art.buildings[slot] = vec![frame; 3];
    }
    client.village.buildings.push(GameplayBuildingState {
        id: EntityId(2),
        player: PlayerId(1),
        kind: BuildingKind::PalisadeGate,
        footprint: TileRect::new(TileCoord::new(98, 100), TileCoord::new(102, 101)),
        construction: ConstructionState::Complete,
        progress_permille: 1000,
        hp: 400,
        max_hp: 400,
        los: 6,
        food_remaining: None,
        training_queue: vec![],
        research: None,
        last_hit: None,
        age: Age::Feudal,
        gate_open: false,
        rally_point: None,
        farm_reseeds: 0,
        garrison: Vec::new(),
        bridge_span: None,
    });
    for (open, progress) in [(false, 1000), (true, 1000), (false, 500)] {
        client.village.buildings[0].gate_open = open;
        client.village.buildings[0].progress_permille = progress;
        assert_eq!(
            units(&client, &scene).len(),
            1,
            "overlapping gate: open={open}, progress={progress}"
        );
    }
    let updates = client.village.gate_art.borrow().updates();
    // Move its projected left bound beyond the viewport, keeping y unchanged.
    client.village.buildings[0].footprint =
        TileRect::new(TileCoord::new(108, 90), TileCoord::new(112, 91));
    assert!(units(&client, &scene).is_empty());
    assert_eq!(client.village.gate_art.borrow().updates(), updates);
    client.canvas.remove();
}

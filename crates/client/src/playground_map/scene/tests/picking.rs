use super::*;
use crate::playground::game::village::picking;
use aoe_core::{ScreenPoint, TileCoord, TileRect};
use aoe_protocol::{Age, BuildingKind, ConstructionState, GameplayBuildingState};
use aoe_rendering::{AtlasAddress, BuildingArt, BuildingPart, GameFrame};

fn building(id: u32, x: i32, y: i32) -> GameplayBuildingState {
    GameplayBuildingState {
        id: EntityId(id),
        player: PlayerId(1),
        kind: BuildingKind::TownCenter,
        footprint: TileRect::from_xywh(x, y, 4, 4),
        construction: ConstructionState::Complete,
        progress_permille: 1000,
        hp: 2400,
        max_hp: 2400,
        los: 8,
        food_remaining: None,
        training_queue: Vec::new(),
        research: None,
        last_hit: None,
        age: Age::Dark,
        gate_open: false,
        rally_point: None,
        farm_reseeds: 0,
        garrison: Vec::new(),
        bridge_span: None,
    }
}

async fn fixture(zoom: f64) -> Client {
    let scene = PreparedScene::new(
        Camera {
            center: [100.0, 100.0],
            zoom,
            viewport: [1280.0, 720.0],
            focus_elevation_meters: 0.0,
        },
        Vec::new(),
    );
    let mut client = client_with_backend(&scene, Some("canvas")).await;
    let frame = GameFrame {
        atlas: AtlasAddress::WHITE,
        size: [200.0, 160.0],
        anchor: [100.0, 140.0],
    };
    let part = BuildingPart {
        frame,
        animation: Vec::new(),
        angles: 1,
        frame_rate: 0.0,
        offset: [0.0; 2],
        depth: 0.0,
    };
    client.art.compositions.push(BuildingArt {
        unit_id: 109,
        footprint: [4.0; 2],
        foundation_material: None,
        parts: vec![
            part.clone(),
            BuildingPart {
                frame: GameFrame {
                    size: [40.0, 50.0],
                    anchor: [20.0, 40.0],
                    ..frame
                },
                offset: [-110.0, 20.0],
                ..part
            },
        ],
        construction: Vec::new(),
    });
    let actor = GameFrame {
        size: [40.0, 80.0],
        anchor: [20.0, 70.0],
        ..frame
    };
    client.art.standing = vec![actor; 5];
    let scout_pose = crate::unit_visuals::Pose::ScoutStand as usize;
    client.art.actors.resize(scout_pose + 1, Vec::new());
    client.art.actors[scout_pose] = vec![actor; 5];
    client.art.resources[0] = vec![actor];
    let scout = client.units.get_mut(&EntityId(1)).unwrap();
    scout.position = WorldPosition::from_tile_center(TileCoord::new(98, 98)).unwrap();
    client.village.buildings.push(building(8, 98, 98));
    client
}

fn click(client: &mut Client, offset: [f64; 2]) {
    let point = ScreenPoint {
        x: 640.0 + offset[0] * client.camera.zoom,
        y: 360.0 + offset[1] * client.camera.zoom,
    };
    let unit = picking::unit(client, point);
    picking::click(client, point, unit);
}

#[wasm_bindgen_test]
async fn house_picking_keeps_the_displayed_workers_bounds_and_depth_until_the_next_frame() {
    for (displayed, incoming, foreground) in [
        (TileCoord::new(101, 101), TileCoord::new(98, 98), true),
        (TileCoord::new(98, 98), TileCoord::new(101, 101), false),
    ] {
        let mut client = fixture(1.0).await;
        client.art.compositions[0].unit_id = 70;
        client.art.compositions[0].footprint = [2.0; 2];
        client.art.village[0] = client.art.standing.clone();
        client.village.buildings[0].kind = BuildingKind::House;
        client.village.buildings[0].footprint = TileRect::from_xywh(99, 99, 2, 2);
        let worker = client.units.get_mut(&EntityId(1)).unwrap();
        worker.kind = aoe_protocol::UnitKind::Villager;
        worker.position = WorldPosition::from_tile_center(displayed).unwrap();
        client.canvas.set_width(1280);
        client.canvas.set_height(720);
        let scene = prepare(&client);
        let actors = units(&client, &scene);
        let camera = SceneCamera {
            center: client.camera.center,
            zoom: client.camera.zoom,
            viewport: client.camera.viewport,
            focus_elevation_meters: client.camera.focus_elevation_meters,
        };
        assert!(
            crate::playground::game::map::render_cache::render_if_changed(
                &mut client,
                &scene,
                &actors,
                &[],
                camera,
                0,
                false,
            )
            .unwrap()
        );

        // A network/interpolation update can arrive while the GPU throttles
        // submissions. AoE2 picking must still follow the displayed worker,
        // including its body rectangle and roof ordering, not only its feet.
        client.units.get_mut(&EntityId(1)).unwrap().position =
            WorldPosition::from_tile_center(incoming).unwrap();
        click(&mut client, [0.0, 20.0]);
        if foreground {
            assert_eq!(client.selected.units, [EntityId(1)]);
        } else {
            assert_eq!(client.selected.building_id, Some(EntityId(8)));
        }
        client.canvas.remove();
    }
}

#[wasm_bindgen_test]
async fn town_center_roof_center_and_bottom_left_annex_select_the_rendered_building() {
    for zoom in [0.5, 1.0, 2.0] {
        let mut client = fixture(zoom).await;
        // Native main body reaches 187 px above and 27 px below its hotspot;
        // the left annex is offset outside the ground footprint.
        for offset in [[0.0, -150.0], [0.0, -90.0], [0.0, 0.0], [-145.0, 20.0]] {
            click(&mut client, offset);
            assert_eq!(
                client.selected.building_id,
                Some(EntityId(8)),
                "zoom {zoom}, {offset:?}"
            );
            assert!(client.selected.units.is_empty());
        }
        // The upper click really overlaps the scout's body behind the roof.
        client.village.buildings.clear();
        click(&mut client, [0.0, -90.0]);
        assert_eq!(client.selected.units, [EntityId(1)]);
        client.canvas.remove();
    }
}

#[wasm_bindgen_test]
async fn ground_farms_remain_selectable_and_do_not_hide_units() {
    let mut client = fixture(1.0).await;
    client.village.buildings[0].kind = BuildingKind::Farm;
    client.village.buildings[0].footprint = TileRect::from_xywh(99, 99, 3, 3);
    click(&mut client, [0.0, 0.0]);
    assert_eq!(client.selected.building_id, Some(EntityId(8)));
    client.units.get_mut(&EntityId(1)).unwrap().position =
        WorldPosition::from_tile_center(TileCoord::new(100, 100)).unwrap();
    click(&mut client, [0.0, 32.0]);
    assert_eq!(client.selected.units, [EntityId(1)]);
    client.canvas.remove();
}

#[wasm_bindgen_test]
async fn building_selection_obeys_draw_depth_for_units_resources_and_other_buildings() {
    let mut client = fixture(1.0).await;
    let scout = client.units.get_mut(&EntityId(1)).unwrap();
    scout.position = WorldPosition::from_tile_center(TileCoord::new(101, 101)).unwrap();
    click(&mut client, [0.0, 20.0]);
    assert_eq!(
        client.selected.units,
        [EntityId(1)],
        "a foreground scout wins"
    );
    client.units.clear();
    let berry =
        aoe_map::ResourceNode::starting(TileCoord::new(98, 98), aoe_map::ResourceKind::Food)
            .unwrap();
    client.resources.apply(aoe_protocol::ResourceState {
        snapshot_page: None,
        subscription_revision: 1,
        from_revision: None,
        revision: 0,
        changes: vec![aoe_protocol::ResourceAmount {
            id: berry.id,
            remaining: berry.initial_amount,
        }],
    });
    client.fog.apply(aoe_protocol::FogState {
        revision: 1,
        tick: aoe_core::Tick(0),
        region: TileRect::from_xywh(0, 0, 1, 1),
        tiles: vec![],
        overview: vec![],
        disabled: true,
    });
    click(&mut client, [0.0, -90.0]);
    assert_eq!(
        client.selected.building_id,
        Some(EntityId(8)),
        "roof hides berries behind it"
    );
    let foreground =
        aoe_map::ResourceNode::starting(TileCoord::new(100, 101), aoe_map::ResourceKind::Food)
            .unwrap();
    client.resources.apply(aoe_protocol::ResourceState {
        snapshot_page: None,
        subscription_revision: 1,
        from_revision: None,
        revision: 1,
        changes: vec![aoe_protocol::ResourceAmount {
            id: foreground.id,
            remaining: foreground.initial_amount,
        }],
    });
    click(&mut client, [-64.0, 20.0]);
    assert_eq!(
        client.selected.resource_id,
        Some(foreground.id),
        "foreground berries win over the building"
    );
    client.resources.clear();
    client.village.buildings.push(building(9, 99, 99));
    client.village.buildings.reverse();
    click(&mut client, [0.0, -40.0]);
    assert_eq!(
        client.selected.building_id,
        Some(EntityId(9)),
        "depth wins over storage order"
    );
    client.village.buildings[0].footprint = client.village.buildings[1].footprint;
    click(&mut client, [0.0, -40.0]);
    assert_eq!(
        client.selected.building_id,
        Some(EntityId(9)),
        "equal depths follow the renderer's entity ID tie break"
    );
    client.canvas.remove();
}

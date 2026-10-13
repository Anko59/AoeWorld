use super::*;
use aoe_core::{EntityId, PlayerId, WorldConfig, WorldPosition};
use aoe_protocol::GameplayUnitState;
use aoe_rendering::{GameArt, GameRenderer};
use std::collections::BTreeMap;
use wasm_bindgen_test::wasm_bindgen_test;

#[path = "budget.rs"]
mod budget;
#[path = "fixture.rs"]
mod fixture;

fn contact_scene() -> PreparedScene {
    let generator = fixture::plane(-432, -432);
    let source = generator
        .landscape_point_with_cancel(aoe_core::TileCoord::new(282, 603), &|| false)
        .unwrap()
        .unwrap()
        .tile
        .terrain;
    assert!(source.passable);
    assert_eq!(source.surface.corner_game_height_levels, [79, 79, 78, 79]);
    let terrain = fixture::region(&generator, 270..295, 590..615);
    for position in [[281.5, 602.5], [282.5, 602.5], [281.5, 603.5]] {
        let ledge = terrain.iter().find(|s| s.position == position).unwrap();
        assert_eq!(ledge.surface.kind, SceneTerrainSurface::CLIFF_LEDGE);
        assert_eq!(ledge.surface.corner_game_height_levels, [80; 4]);
    }
    PreparedScene::new(
        Camera {
            center: [282.5, 603.5],
            zoom: 1.0,
            viewport: [512.0; 2],
            focus_elevation_meters: 79.0,
        },
        terrain,
    )
}

#[wasm_bindgen_test]
fn review_f1_passable_center_contacts_source_ground_below_the_cliff() {
    let scene = contact_scene();
    assert_eq!(scene.height([282.5, 603.5]), Some(78.5));
}

#[wasm_bindgen_test]
fn review_f1_rim_cannot_cover_the_neighboring_passable_center() {
    let scene = contact_scene();
    let cliff: Vec<_> = scene
        .triangles
        .iter()
        .filter(|t| !t.skirt && t.tile == [281, 602])
        .collect();
    assert_eq!(cliff.len(), 2);
    let corner = cliff
        .iter()
        .flat_map(|t| t.points)
        .find(|p| p.world[0] > 281.9 && p.world[1] > 602.9)
        .unwrap();
    for axis in 0..2 {
        assert!(
            (corner.world[axis] - [282.0, 603.0][axis]).abs() < 0.5,
            "rim crosses tile center: {:?}",
            corner.world
        );
    }
    // Every point of the central walkable square stays outside the ledge tops.
    for x in [282.3, 282.5, 282.7] {
        for y in [603.3, 603.5, 603.7] {
            assert!(scene.height([x, y]).unwrap() < 80.0);
        }
    }
}

#[wasm_bindgen_test]
async fn review_f1_unit_dto_stays_on_source_ground_instead_of_cliff_top() {
    let scene = contact_scene();
    let document = web_sys::window().unwrap().document().unwrap();
    let canvas = document
        .create_element("canvas")
        .unwrap()
        .dyn_into()
        .unwrap();
    let (renderer, canvas) = GameRenderer::new(canvas).await.unwrap();
    let id = EntityId(1);
    let client = Client {
        document,
        canvas,
        renderer,
        art: GameArt {
            walking: Vec::new(),
            standing: Vec::new(),
            grass: Vec::new(),
            terrain: Default::default(),
            terrain_topology: [None; 12],
            water_depth: Default::default(),
            resources: Default::default(),
            tree_shadows: Vec::new(),
            tree_families: Default::default(),
            scenery: Default::default(),
        },
        socket: None,
        connection_id: 0,
        camera: scene.camera,
        config: WorldConfig::default(),
        primary: Some(id),
        role: None,
        map_content_hash: None,
        surface_fixture: false,
        focus_map_hash: None,
        resources: Default::default(),
        terrain_scene: RefCell::new(None),
        rendered_frame: Default::default(),
        terrain_chunks: BTreeMap::new(),
        terrain_height_bounds: None,
        terrain_bounds: Default::default(),
        terrain_resident_height_bounds: None,
        terrain_requests: Default::default(),
        token: None,
        revision: 0,
        sent_region: None,
        last_subscribe: 0.0,
        next_sequence: 1,
        units: BTreeMap::from([(
            id,
            GameplayUnitState {
                id,
                player: PlayerId(1),
                position: WorldPosition::from_tile_center(aoe_core::TileCoord::new(282, 603))
                    .unwrap(),
                moving: false,
                planning: false,
                facing: 0,
            },
        )]),
        presentation: Default::default(),
        selected: Some(id),
        grid: false,
        drag: None,
        pointer: None,
        last_frame: 0.0,
        status: String::new(),
    };
    let units = units(&client, &scene);
    assert_eq!(units.len(), 1);
    assert_eq!(units[0].position, [282.5, 603.5]);
    assert_eq!(units[0].elevation_meters, 78.5);
    assert_eq!(
        client
            .camera
            .world_to_screen_at_height(units[0].position, units[0].elevation_meters),
        client
            .camera
            .world_to_screen_at_height([282.5, 603.5], 78.5)
    );
}

//! World objects keep the original art's size relative to a tile and are not
//! cut off by the ground they stand on.
use super::*;
use aoe_core::EntityId;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::wasm_bindgen_test;

/// The original art is drawn for a 96 by 48 pixel tile; this world's is 128 by 64.
const WORLD: f64 = 128.0 / 96.0;

#[wasm_bindgen_test]
fn missing_wildlife_art_keeps_neutral_animals_distinct_from_player_unit_markers() {
    use crate::SceneUnitKind::*;
    for kind in [
        NeutralSheepStand,
        NeutralSheepWalk,
        NeutralSheepDie,
        NeutralSheepDecay,
        DeerStand,
        DeerWalk,
        DeerDie,
        DeerDecay,
        BoarStand,
        BoarWalk,
        BoarAttack,
        BoarDie,
        BoarDecay,
        WolfStand,
        WolfWalk,
        WolfRun,
        WolfAttack,
        WolfDie,
        WolfDecay,
        SheepStand,
        SheepWalk,
        SheepDie,
        SheepDecay,
        Villager,
        MilitiaStand,
    ] {
        let unit = SceneUnit {
            kind,
            id: EntityId(7),
            position: [10.5; 2],
            moving: false,
            facing: 0,
            selected: false,
            elevation_meters: 0.0,
        };
        let drawn = world_sprite_frames(&art(), &ground(), &[], &[unit], camera(1.0), 0);
        assert_eq!(drawn.len(), 1, "{kind:?}");
        let player = matches!(
            kind,
            SheepStand | SheepWalk | SheepDie | SheepDecay | Villager | MilitiaStand
        );
        assert_eq!(
            drawn[0].0.color,
            if player {
                [0.25, 0.57, 0.96, 1.0]
            } else {
                [0.75, 0.75, 0.75, 1.0]
            },
            "{kind:?}",
        );
        near(rect(&drawn[0].0), [249.0, 226.0, 263.0, 258.0]);
    }
}

fn frame(size: [f32; 2], anchor: [f32; 2]) -> GameFrame {
    GameFrame {
        atlas: crate::AtlasAddress {
            page: 0,
            uv: [0.25, 0.25, 0.01, 0.01],
        },
        size,
        anchor,
    }
}

/// Hotspots as in the reviewed pack: a gold pile's is in its middle.
fn art() -> GameArt {
    GameArt {
        actors: Vec::new(),
        buildings: std::array::from_fn(|_| Vec::new()),
        village: Default::default(),
        fishing: Default::default(),
        compositions: Vec::new(),
        foundation: Default::default(),
        walking: vec![frame([40.0, 77.0], [23.0, 60.0]); 50],
        standing: vec![frame([40.0, 77.0], [23.0, 60.0]); 50],
        grass: Vec::new(),
        terrain: std::array::from_fn(|_| Vec::new()),
        terrain_topology: [None; 12],
        resources: [
            vec![frame([71.0, 50.0], [42.0, 39.0])],
            vec![frame([119.0, 143.0], [52.0, 142.0])],
            vec![frame([82.0, 40.0], [38.0, 20.0])],
            vec![frame([71.0, 38.0], [39.0, 27.0])],
        ],
        tree_shadows: Vec::new(),
        tree_families: Default::default(),
        scenery: Default::default(),
        water_depth: Default::default(),
    }
}

fn camera(zoom: f64) -> SceneCamera {
    SceneCamera {
        center: [10.5; 2],
        zoom,
        viewport: [512.0; 2],
        focus_elevation_meters: 0.0,
    }
}

fn resource(kind: u8) -> SceneResource {
    SceneResource {
        id: u64::from(kind) + 1,
        position: [10.5; 2],
        kind,
        visual_variant: 0,
        visual_family: u8::from(kind == 1),
        elevation_meters: 0.0,
    }
}

fn ground() -> Vec<SceneTerrain> {
    (0..9)
        .map(|index| SceneTerrain {
            appearance: None,
            position: [9.5 + f64::from(index % 3), 9.5 + f64::from(index / 3)],
            material: 0,
            elevation_meters: 0.0,
            surface: SceneTerrainSurface::flat(0.0),
        })
        .collect()
}

/// Left, top, right and bottom of an emitted sprite in screen pixels.
fn rect(sprite: &Sprite) -> [f64; 4] {
    let [x, y] = sprite.position.map(f64::from);
    let [half_width, half_height] = sprite.radius.map(f64::from);
    [
        (x - half_width + 1.0) * 256.0,
        (1.0 - y - half_height) * 256.0,
        (x + half_width + 1.0) * 256.0,
        (1.0 - y + half_height) * 256.0,
    ]
}

fn near(actual: [f64; 4], expected: [f64; 4]) {
    for (actual, expected) in actual.into_iter().zip(expected) {
        assert!(
            (actual - expected).abs() < 1e-3,
            "{actual} is not {expected}"
        );
    }
}

#[wasm_bindgen_test]
fn resources_keep_native_size_relative_to_the_tile_with_the_hotspot_on_its_centre() {
    let art = art();
    for zoom in [1.0, 0.5] {
        let camera = camera(zoom);
        for kind in 0..4_u8 {
            let native = art.resources[usize::from(kind)][0];
            let [width, height] = native.size.map(f64::from);
            let [anchor_x, anchor_y] = native.anchor.map(f64::from);
            let scale = WORLD * zoom;
            // The tile centre is the screen centre; the hotspot sits on it.
            let expected = [
                256.0 - anchor_x * scale,
                256.0 - anchor_y * scale,
                256.0 + (width - anchor_x) * scale,
                256.0 + (height - anchor_y) * scale,
            ];
            let drawn = world_sprite_frames(&art, &ground(), &[resource(kind)], &[], camera, 0);
            let (body, emitted, _, _) = drawn.last().unwrap();
            near(rect(body), expected);
            near(
                [
                    emitted.size[0],
                    emitted.size[1],
                    emitted.anchor[0],
                    emitted.anchor[1],
                ]
                .map(f64::from),
                [width, height, anchor_x, anchor_y].map(|pixels| pixels * scale),
            );
            // The client culls with the same rectangle.
            let selected = scene_resource_frame(&art, resource(kind)).unwrap();
            near(
                resource_sprite_bounds(resource(kind), selected, camera).unwrap(),
                expected,
            );
            // A gold pile spans 82 of its tile's 96 native pixels at any zoom.
            let tile_width = 128.0 * zoom;
            assert!(((expected[2] - expected[0]) / tile_width - width / 96.0).abs() < 1e-6);
        }
    }
}

#[wasm_bindgen_test]
fn units_are_drawn_at_the_same_native_scale() {
    let unit = SceneUnit {
        kind: Default::default(),
        id: EntityId(7),
        position: [10.5; 2],
        moving: false,
        facing: 0,
        selected: false,
        elevation_meters: 0.0,
    };
    let drawn = world_sprite_frames(&art(), &ground(), &[], &[unit], camera(1.0), 0);
    let [left, top, right, bottom] = rect(&drawn.last().unwrap().0);
    assert!((right - left - 40.0 * WORLD).abs() < 1e-3);
    near(
        [top, bottom, 0.0, 0.0],
        [256.0 - 60.0 * WORLD, 256.0 + 17.0 * WORLD, 0.0, 0.0],
    );
}

#[wasm_bindgen_test]
fn objects_share_one_clearance_that_covers_the_art_below_a_hotspot() {
    let unit = SceneUnit {
        kind: Default::default(),
        id: EntityId(7),
        position: [10.25, 10.5],
        moving: false,
        facing: 0,
        selected: false,
        elevation_meters: 0.0,
    };
    // The deepest reviewed art: gold variant 3 reaches 32 native rows below
    // its hotspot (52 - 20), not the shared fixture's 20.
    let mut art = art();
    art.resources[2] = vec![frame([82.0, 52.0], [38.0, 20.0])];
    let drawn = world_sprite_frames(&art, &ground(), &[resource(2)], &[unit], camera(1.0), 0);
    let pile =
        drawn.iter().find(|entry| entry.3 == 3).unwrap().2 - surface_depth([10.5, 10.5, 0.0]);
    let rider =
        drawn.iter().find(|entry| entry.3 == 7).unwrap().2 - surface_depth([10.25, 10.5, 0.0]);
    // Equal offsets keep objects ordered among themselves by where they stand.
    assert_eq!(pile, rider);
    // Flat ground is one depth unit nearer per 32 pixels down the screen; the
    // deepest reviewed art (gold variant 3) reaches 32 native pixels below.
    assert!(pile * 32.0 >= 32.0 * WORLD, "clearance {pile}");
    // A one-level rise in front of an object (two depth units) still hides it.
    assert!(pile < 2.0, "clearance {pile}");
    // The stated overhang is really covered: at zoom 1 the pile's lowest row
    // lies 32 * WORLD pixels below the hotspot, where flat ground has gained
    // 32 * WORLD / 32 = WORLD depth units, and it must still be drawn there.
    let document = web_sys::window().unwrap().document().unwrap();
    let canvas = document
        .create_element("canvas")
        .unwrap()
        .dyn_into::<HtmlCanvasElement>()
        .unwrap();
    canvas.set_width(512);
    canvas.set_height(512);
    let context = context(&canvas).unwrap();
    let mut renderer = GameRenderer::Canvas {
        canvas: canvas.clone(),
        context: context.clone(),
        atlas: [None, None, None, None],
        // White art over flat-coloured ground.
        source_atlas: vec![255; crate::GAME_ATLAS_BYTES],
        presentation: CanvasPresentation::new(512, 512),
    };
    renderer
        .render_world(&art, &ground(), &[resource(2)], &[], camera(1.0), 0, false)
        .unwrap();
    let white =
        |x: f64, y: f64| context.get_image_data(x, y, 1.0, 1.0).unwrap().data().0 == [255; 4];
    let lowest_row = (256.0 + 32.0 * WORLD).floor();
    assert!(
        white(256.0, lowest_row),
        "the lowest row of the deepest reviewed art is drawn over nearer ground"
    );
    assert!(
        !white(256.0, lowest_row + 4.0),
        "nothing of the pile is drawn below its lowest row"
    );
}

#[wasm_bindgen_test]
fn canvas_draws_a_whole_pile_over_its_tile() {
    let document = web_sys::window().unwrap().document().unwrap();
    let canvas = document
        .create_element("canvas")
        .unwrap()
        .dyn_into::<HtmlCanvasElement>()
        .unwrap();
    canvas.set_width(512);
    canvas.set_height(512);
    let context = context(&canvas).unwrap();
    let mut renderer = GameRenderer::Canvas {
        canvas: canvas.clone(),
        context: context.clone(),
        atlas: [None, None, None, None],
        // White art over flat-coloured ground.
        source_atlas: vec![255; crate::GAME_ATLAS_BYTES],
        presentation: CanvasPresentation::new(512, 512),
    };
    let camera = camera(1.0);
    renderer
        .render_world(&art(), &ground(), &[resource(2)], &[], camera, 0, false)
        .unwrap();
    let white =
        |x: f64, y: f64| context.get_image_data(x, y, 1.0, 1.0).unwrap().data().0 == [255; 4];
    // 82 by 40 native pixels with the hotspot at (38, 20), drawn at 4/3: the
    // pile reaches 26 pixels above and below the tile centre, 50 left, 58 right.
    assert!(white(256.0, 232.0), "top half");
    assert!(
        white(256.0, 280.0),
        "half below the hotspot, over nearer ground"
    );
    assert!(white(208.0, 256.0) && white(312.0, 256.0), "full width");
    assert!(
        !white(256.0, 226.0) && !white(256.0, 286.0),
        "no taller than 4/3 native"
    );
    assert!(
        !white(202.0, 256.0) && !white(318.0, 256.0),
        "no wider than 4/3 native"
    );
}

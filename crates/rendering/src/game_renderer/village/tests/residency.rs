use super::*;
use wasm_bindgen::JsCast;

mod fallbacks;
mod growth;

#[wasm_bindgen_test]
fn native_lookahead_wraps_inside_the_current_facing_without_changing_the_display_cell() {
    let mut art = art();
    let frame = art.village[1][0];
    art.actors = vec![Vec::new(); 135];
    art.actors[0] = (1..=480)
        .map(|id| GameFrame {
            stream: id,
            ..frame
        })
        .collect();
    let unit = unit(SceneUnitKind::actor(0)).with_actor_frame((29 << 4) | 15);
    assert_eq!(
        crate::scene_native_frames(&art, &[unit], camera(), 0),
        vec![480]
    );
    assert_eq!(
        crate::scene_native_prefetch(&art, &[unit], camera(), 0),
        vec![451, 452]
    );
}

#[wasm_bindgen::prelude::wasm_bindgen(inline_js = "
export function native_patch_gl_pixel(canvas) {
 const gl=canvas.getContext('webgl2'); gl.finish();
 const pixel=new Uint8Array(4); gl.readPixels(64,63,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixel);
 if(gl.getError()!==gl.NO_ERROR)throw Error('native patch read failed'); return pixel;
}
")]
extern "C" {
    fn native_patch_gl_pixel(canvas: &web_sys::HtmlCanvasElement) -> Vec<u8>;
}

#[wasm_bindgen_test]
async fn native_patch_pixels_reach_every_backend_without_replacing_the_atlas() {
    for backend in ["canvas", "webgl2", "webgpu"] {
        let canvas: web_sys::HtmlCanvasElement = web_sys::window()
            .unwrap()
            .document()
            .unwrap()
            .create_element("canvas")
            .unwrap()
            .dyn_into()
            .unwrap();
        canvas.set_width(128);
        canvas.set_height(128);
        web_sys::window()
            .unwrap()
            .document()
            .unwrap()
            .body()
            .unwrap()
            .append_child(&canvas)
            .unwrap();
        let (mut renderer, canvas) = crate::GameRenderer::new_with_backend(canvas, Some(backend))
            .await
            .unwrap();
        assert_eq!(
            renderer.backend(),
            if backend == "canvas" {
                "canvas2d"
            } else {
                backend
            }
        );
        let mut upload = renderer.begin_game_atlas(10).unwrap();
        for layer in 0..10 {
            upload.page_buffer(layer).unwrap();
            renderer.write_game_atlas_page(&mut upload, layer).unwrap();
        }
        renderer.finish_game_atlas(upload).unwrap();
        let mut art = art();
        let mut frame = art.village[1][0];
        frame.size = [32.0; 2];
        frame.anchor = [16.0; 2];
        frame.stream = 1;
        art.actors = vec![Vec::new(); 135];
        art.actors[0] = vec![frame; 16];
        art.resident = vec![Some(crate::AtlasAddress {
            page: 8,
            uv: [2.0 / 2048.0, 2.0 / 2048.0, 1.0 / 2048.0, 1.0 / 2048.0],
        })];
        let camera = SceneCamera {
            viewport: [128.0; 2],
            ..camera()
        };
        let before = renderer.memory_usage();
        for colour in [[255, 0, 0, 255], [0, 255, 0, 255]] {
            renderer
                .update_game_atlas(8, [2, 2], [1, 1], &colour)
                .unwrap();
            assert!(
                renderer
                    .render_world(
                        &art,
                        &[],
                        &[],
                        &[unit(SceneUnitKind::actor(0))],
                        camera,
                        0,
                        false
                    )
                    .unwrap()
            );
            let pixel = match &renderer {
                crate::GameRenderer::Canvas { context, .. } => {
                    context
                        .get_image_data(64.0, 64.0, 1.0, 1.0)
                        .unwrap()
                        .data()
                        .0
                }
                crate::GameRenderer::WebGl(_) => native_patch_gl_pixel(&canvas),
                crate::GameRenderer::WebGpu(gpu) => {
                    crate::web::tests::terrain_blend::read_pixel(gpu, 2, [64, 64])
                        .await
                        .to_vec()
                }
            };
            assert_eq!(pixel, colour, "{backend}");
        }
        assert_eq!(before[0], renderer.memory_usage()[0]);
        for (page, origin, size, bytes) in [
            (10, [0, 0], [1, 1], 4),
            (8, [2048, 0], [1, 1], 4),
            (8, [0, 0], [0, 1], 0),
            (8, [0, 0], [2, 1], 4),
            (8, [u32::MAX, 0], [1, 1], 4),
        ] {
            assert!(
                renderer
                    .update_game_atlas(page, origin, size, &vec![0; bytes])
                    .is_err()
            );
        }
        if let crate::GameRenderer::WebGpu(gpu) = &renderer {
            gpu.device.destroy();
        }
        canvas.remove();
    }
}

#[wasm_bindgen_test]
fn native_residency_changes_only_the_address_and_preserves_facing_geometry() {
    let mut art = art();
    let mut native = art.village[1][0];
    native.stream = 1;
    native.size = [91.0, 149.0];
    native.anchor = [-3.0, 160.0];
    art.actors = vec![Vec::new(); 135];
    art.actors[0] = vec![native; 16 * 30];
    art.resident = vec![None];
    let unit = unit(SceneUnitKind::actor(0));
    let demands = crate::scene_native_frames(&art, &[unit], camera(), 0);
    assert_eq!(demands, vec![1]);
    assert!(world_sprite_frames(&art, &[], &[], &[unit], camera(), 0).is_empty());
    let address = crate::AtlasAddress {
        page: 8,
        uv: [0.25, 0.5, 91.0 / 2048.0, 149.0 / 2048.0],
    };
    art.resident[0] = Some(address);
    let emitted = world_sprite_frames(&art, &[], &[], &[unit], camera(), 0);
    let body = emitted.last().unwrap();
    assert_eq!(body.1.atlas, address);
    assert_eq!(body.1.stream, 1);
    assert_eq!(body.0.pages[0], 8);
    let before = body.0.position;
    art.resident[0] = Some(crate::AtlasAddress {
        page: 10,
        ..address
    });
    let emitted = world_sprite_frames(&art, &[], &[], &[unit], camera(), 0);
    assert_eq!(emitted.last().unwrap().0.position, before);
    assert_eq!(emitted.last().unwrap().0.pages[0], 10);
    assert_eq!(art.actors[0][0].size, [91.0, 149.0]);
    let offscreen = SceneUnit {
        position: [1_000_000.0; 2],
        ..unit
    };
    assert!(crate::scene_native_frames(&art, &[offscreen], camera(), 0).is_empty());
    art.resident[0] = None;
    assert!(world_sprite_frames(&art, &[], &[], &[unit], camera(), 0).is_empty());
}

#[wasm_bindgen_test]
fn native_building_demand_timing_and_destruction_share_immutable_geometry() {
    let mut art = art();
    let mut part = art.compositions[0].parts[0].clone();
    let base = part.frame;
    part.animation = (1..=4).map(|stream| GameFrame { stream, ..base }).collect();
    part.frame = part.animation[0];
    part.frame_rate = 0.025;
    part.replay_delay = 0.05;
    art.compositions[0].parts = vec![part.clone()];
    let building = unit(SceneUnitKind::TownCenter);
    assert_eq!(
        crate::scene_native_frames(&art, &[building], camera(), 50),
        vec![3]
    );
    assert!(
        world_sprite_frames(&art, &[], &[], &[building], camera(), 50).is_empty(),
        "missing resident art draws no stale atlas rectangle"
    );
    assert!(
        crate::scene_building_visible(&art, building, camera(), 50),
        "native geometry is visible before residency"
    );
    let mut offscreen = building;
    offscreen.position = [10000.0; 2];
    assert!(crate::scene_native_frames(&art, &[offscreen], camera(), 50).is_empty());
    art.resident = (0..5)
        .map(|index| {
            Some(crate::AtlasAddress {
                page: 8,
                uv: [index as f32 / 32.0, 0.0, 0.01, 0.01],
            })
        })
        .collect();
    let shown = world_sprite_frames(&art, &[], &[], &[building], camera(), 50);
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].1.atlas, art.resident[2].unwrap());
    // DAT replay delay holds the last cell; the loop restarts at 150ms.
    assert_eq!(
        crate::scene_native_frames(&art, &[building], camera(), 125),
        vec![4]
    );
    assert_eq!(
        crate::scene_native_frames(&art, &[building], camera(), 150),
        vec![1]
    );
    let mut rubble = part.clone();
    rubble.animation.clear();
    rubble.frame = GameFrame { stream: 5, ..base };
    art.compositions[0].states = vec![(0, vec![part]), (1, vec![rubble])];
    art.building_clocks = vec![(building.id.0, 30.0)];
    let dying = SceneUnit {
        visual: crate::SceneUnitVisual::new(true),
        ..building
    }
    .with_actor_frame(3);
    assert_eq!(
        crate::scene_native_frames(&art, &[dying], camera(), 999),
        vec![2]
    );
    art.building_clocks[0].1 = 110.0;
    let dead = dying;
    assert_eq!(
        crate::scene_native_frames(&art, &[dead], camera(), 0),
        vec![5]
    );
    assert_eq!(art.compositions[0].destruction_millis(), 101); // Stored f32 duration is slightly above 25ms.
    art.resident[4] = None;
    assert!(world_sprite_frames(&art, &[], &[], &[dead], camera(), 0).is_empty());
    assert_eq!(
        crate::scene_native_frames(&art, &[dead], camera(), 0),
        vec![5],
        "eviction cannot erase demand"
    );
}

#[wasm_bindgen_test]
fn bridge_and_rubble_recipes_keep_flat_occlusion_and_empty_corpses_disappear() {
    let mut art = art();
    for kind in [SceneUnitKind::BridgeATop, SceneUnitKind::RubbleHouse] {
        art.compositions[0].unit_id = kind.building_id().unwrap();
        let frames = world_sprite_frames(&art, &[], &[], &[unit(kind)], camera(), 0);
        assert!(!frames.is_empty());
        assert!(
            frames
                .iter()
                .all(|(sprite, _, _, _)| sprite.pages[3] & Sprite::OCCLUDER == 0)
        );
    }
    art.compositions[0].unit_id = 45;
    art.compositions[0].states = vec![(0, Vec::new()), (1, Vec::new())];
    let mut dock = unit(SceneUnitKind::Dock);
    dock.visual = crate::SceneUnitVisual::new(true);
    assert!(
        world_sprite_frames(&art, &[], &[], &[dock], camera(), 0).is_empty(),
        "an explicit empty DAT corpse does not become a missing-art marker"
    );
}

#[wasm_bindgen_test]
fn all_environment_parts_use_dat_variants_timing_offsets_and_visible_residency() {
    let mut art = art();
    let base = art.village[1][0];
    let make = |stream| GameFrame { stream, ..base };
    art.environment.push(crate::BuildingArt {
        unit_id: 350,
        footprint: [1.0; 2],
        foundation_material: None,
        construction: Vec::new(),
        damage: Vec::new(),
        states: vec![(0, Vec::new())],
        parts: vec![
            crate::BuildingPart {
                frame: make(1),
                animation: vec![make(1), make(2), make(3)],
                angles: 1,
                frame_rate: 0.0,
                replay_delay: 0.0,
                offset: [-7.0, 11.0],
                depth: 0.0,
            },
            crate::BuildingPart {
                frame: make(4),
                animation: vec![make(4), make(5)],
                angles: 1,
                frame_rate: 0.03125,
                replay_delay: 0.0,
                offset: [0.0; 2],
                depth: 0.0,
            },
        ],
    });
    let r = SceneResource {
        highlight: 0,
        id: 9,
        position: [0.0; 2],
        kind: 1,
        visual_variant: 2,
        visual_family: 2,
        elevation_meters: 0.0,
    };
    assert_eq!(
        crate::scene_resource_native_frames(&art, &[r], camera(), 31),
        vec![3, 4]
    );
    assert_eq!(
        crate::scene_resource_native_frames(&art, &[r], camera(), 32),
        vec![3, 5]
    );
    let mut frames = Vec::new();
    environment::frames(&art, r, 32, &mut |f| frames.push(f));
    assert_eq!(
        frames[0].anchor,
        [base.anchor[0] + 7.0, base.anchor[1] - 11.0]
    );
    let chopped = SceneResource {
        kind: SceneResource::CHOPPED_TREE,
        ..r
    };
    assert!(
        crate::scene_resource_native_frames(&art, &[chopped], camera(), 32).is_empty(),
        "an empty DAT state stays empty"
    );
    let offscreen = SceneResource {
        position: [10000.0; 2],
        ..r
    };
    assert!(crate::scene_resource_native_frames(&art, &[offscreen], camera(), 32).is_empty());
}

#[wasm_bindgen_test]
fn many_kind_pressure_keeps_two_thousand_actors_when_the_next_phase_is_cold() {
    let mut art = art();
    let mut slots = std::collections::BTreeSet::new();
    let kinds: Vec<_> = (0..167)
        .map(SceneUnitKind::actor)
        .filter(|kind| kind.native_directions() && slots.insert(kind.actor_slot().unwrap()))
        .take(64)
        .collect();
    assert_eq!(kinds.len(), 64);
    art.actors = vec![Vec::new(); 167];
    art.actor_rows = vec![16; 167];
    art.resident = vec![None; 64 * 16 * 2];
    let mut resident = 0;
    for (group, kind) in kinds.iter().enumerate() {
        let slot = kind.actor_slot().unwrap();
        art.actors[slot] = (0..32)
            .map(|index| {
                let stream = (group * 32 + index + 1) as u32;
                if index % 2 == 0 {
                    let x = 2 + resident % 10 * 192;
                    let y = 2 + resident / 10 % 10 * 192;
                    art.resident[stream as usize - 1] = Some(crate::AtlasAddress {
                        page: 8 + (resident / 100) as u32 * 2,
                        uv: [
                            x as f32 / 2048.0,
                            y as f32 / 2048.0,
                            188.0 / 2048.0,
                            188.0 / 2048.0,
                        ],
                    });
                    resident += 1;
                }
                GameFrame {
                    stream,
                    atlas: crate::AtlasAddress::new(0, [0.0; 4]),
                    size: if index % 2 == 0 {
                        [188.0; 2]
                    } else {
                        [180.0, 170.0]
                    },
                    anchor: if index % 2 == 0 {
                        [94.0, 188.0]
                    } else {
                        [92.0, 175.0]
                    },
                }
            })
            .collect();
    }
    // Current cells alone exceed the retired four-pair budget; every phase
    // keeps native geometry, even though an alternate phase may be unavailable.
    assert_eq!(resident, 1024);
    assert!(resident * 192 * 192 * 8 > 4 * 2 * 2048 * 2048 * 4);
    let camera = SceneCamera {
        zoom: 0.25,
        viewport: [1280.0, 720.0],
        ..camera()
    };
    let units: Vec<_> = (0..2000)
        .map(|i| {
            let mut actor = unit(kinds[i % kinds.len()]);
            actor.id = aoe_core::EntityId(i as u32 + 1);
            actor.position = [0.5 + (i % 45) as f64 * 0.05, 0.5 + (i / 45) as f64 * 0.05];
            actor.with_actor_frame((i / kinds.len() % 16) as u16)
        })
        .collect();
    let bodies = |units: &[SceneUnit]| {
        super::super::world_sprite_frames(&art, &[], &[], units, camera, 0)
            .into_iter()
            .filter(|(sprite, _, _, _)| sprite.color[3] == 1.0)
            .map(|(sprite, frame, _, id)| {
                (id, (sprite.uv, sprite.pages[0], frame.size, frame.anchor))
            })
            .collect::<std::collections::BTreeMap<_, _>>()
    };
    let before = bodies(&units);
    assert_eq!(before.len(), 2000);
    let next: Vec<_> = units
        .iter()
        .map(|u| u.with_actor_frame(u.actor_frame() | 1 << 4))
        .collect();
    let desired = crate::scene_native_frames(&art, &next, camera, 0);
    assert_eq!(desired.len(), 1024);
    assert!(
        desired.iter().all(|id| id % 2 == 0),
        "demand must retain the requested phase"
    );
    let after = bodies(&next);
    assert_eq!(
        after.len(),
        before.len(),
        "a cold phase must retain every visible actor"
    );
    for (id, expected) in before {
        assert_eq!(
            after.get(&id),
            Some(&expected),
            "previous geometry for actor {id}"
        );
    }
}

//! The HUD's native-art flag (`data-village-art`) reports art the scene draws:
//! the village's DAT compositions. A trial pack has those and no unit art, so
//! neither trial villager sheets nor native actors can stand for it.
use super::*;
use aoe_rendering::{BuildingArt, BuildingDamage, BuildingPart};

fn native_art_flag(client: &Client) -> f64 {
    let previous = js_sys::Function::new_no_args(
        "const old=window.aoeInterface; window.aoeInterface={numeric:v=>{window.__artHUD=Array.from(v)},details:()=>{}}; return old;",
    )
    .call0(&wasm_bindgen::JsValue::UNDEFINED)
    .unwrap();
    crate::playground::game::village::hud::update(client);
    let window = web_sys::window().unwrap();
    let captured = js_sys::Reflect::get(&window, &"__artHUD".into()).unwrap();
    js_sys::Reflect::set(&window, &"aoeInterface".into(), &previous).unwrap();
    js_sys::Reflect::delete_property(&window, &"__artHUD".into()).unwrap();
    js_sys::Array::from(&captured).get(11).as_f64().unwrap()
}

/// A client on the contact scene without any art.
async fn artless_client() -> Client {
    let scene = contact_scene();
    let client = client_with_backend(&scene, Some("canvas")).await;
    client.canvas.set_width(512);
    client.canvas.set_height(512);
    *client.terrain_scene.borrow_mut() = Some(Rc::new(scene));
    client
}

fn frame() -> GameFrame {
    GameFrame {
        stream: 0,
        atlas: AtlasAddress::WHITE,
        size: [1.0; 2],
        anchor: [0.0; 2],
    }
}

/// A recipe whose every part list is empty, as the loader leaves one whose
/// sheets the pack lacks: each list is there and none of them has art.
fn partless(unit_id: i32) -> BuildingArt {
    BuildingArt {
        unit_id,
        footprint: [4.0; 2],
        foundation_material: None,
        parts: Vec::new(),
        construction: Vec::new(),
        states: vec![(0, Vec::new()), (1, Vec::new()), (5, Vec::new())],
        damage: [25, 50]
            .map(|damage_percent| BuildingDamage {
                damage_percent,
                apply_mode: 0,
                parts: Vec::new(),
            })
            .into(),
    }
}

#[wasm_bindgen_test]
async fn native_art_flag_follows_the_drawn_village_instead_of_trial_villager_sheets() {
    let mut client = artless_client().await;
    // All nine villager sheets and every actor pose, but no composition.
    client.art.village = std::array::from_fn(|_| vec![frame(); 75]);
    let sprites = &aoe_assets::catalog::units::SPRITES;
    client.art.actors = sprites.iter().map(|s| vec![frame(); s.rows()]).collect();
    client.art.actor_rows = sprites.iter().map(|s| s.rows()).collect();
    let markers = native_art_flag(&client);
    client.art.compositions.push(BuildingArt {
        parts: vec![BuildingPart {
            frame: frame(),
            animation: Vec::new(),
            angles: 1,
            frame_rate: 0.0,
            replay_delay: 0.0,
            offset: [0.0; 2],
            depth: 0.0,
        }],
        ..partless(109)
    });
    let native = native_art_flag(&client);
    client.canvas.remove();
    assert_eq!(markers, 0.0, "a pack without village art draws markers");
    assert_eq!(native, 1.0, "a native Town Center is native village art");
}

/// A recipe whose sprites are all absent keeps its entry without parts: the
/// village then draws markers, so it is no native art.
#[wasm_bindgen_test]
async fn compositions_without_any_part_are_not_native_art() {
    let mut client = artless_client().await;
    client.art.compositions = vec![partless(109), partless(70)];
    let flag = native_art_flag(&client);
    client.canvas.remove();
    assert_eq!(flag, 0.0, "recipes without any part draw markers");
}

//! Trial-like and DE-like packs without original pixels, and their server.
//! Every sheet owns one grey texel, so its drawn cells stay recognisable.
use super::super::*;
use aoe_assets::catalog::units::{Pose, SPRITES};
use serde_json::{Value, json};

/// Source page, archive, SLP and frame count of a trial-like pack. The row
/// number is the sheet's column on its page and, plus one, its grey level.
pub(super) const SHEETS: &[(&str, &str, u32, u32)] = &[
    ("ground", "terrain", 15008, 100),
    ("ground", "terrain", 15007, 100),
    ("ground", "terrain", 15000, 100),
    ("ground", "terrain", 15010, 100),
    ("ground", "terrain", 15002, 100),
    ("ground", "terrain", 15011, 10),
    ("ground", "terrain", 15016, 100),
    ("ground", "terrain", 15015, 100),
    ("ground", "terrain", 15014, 100),
    ("ground", "terrain", 15006, 100),
    ("ground", "terrain", 15009, 100),
    ("ground", "terrain", 15001, 100),
    ("ground", "terrain", 15017, 100),
    ("ground", "terrain", 15033, 100),
    ("ground", "terrain", 15024, 64),
    ("ground", "terrain", 15026, 100),
    ("objects", "graphics", 4652, 14),
    ("objects", "graphics", 2296, 14),
    ("objects", "graphics", 2560, 4),
    ("objects", "graphics", 4479, 7),
    ("objects", "graphics", 4482, 7),
    ("objects", "graphics", 4654, 9),
    ("objects", "graphics", 4653, 13),
    ("objects", "graphics", 226, 25),
    ("objects", "graphics", 1034, 7),
    ("objects", "graphics", 1035, 3),
    ("objects", "graphics", 1252, 2),
    ("projectiles", "graphics", ARROW, 176),
    ("projectiles", "graphics", JAVELIN, 170),
    ("house", "graphics", HOUSE, 3),
    ("fire-25", "graphics", FIRE_25, 2),
    ("fire-50", "graphics", FIRE_50, 2),
    // No renderer reads any sheet below.
    ("cavalry", "graphics", 3008, 50),
    ("cavalry", "graphics", 3004, 50),
    ("villagers", "graphics", 1479, 75),
    ("villagers", "graphics", 1484, 75),
    ("villagers", "graphics", 1535, 75),
    ("villagers", "graphics", 1536, 75),
    ("villagers", "graphics", 2589, 75),
    ("villagers", "graphics", 2592, 75),
    ("villagers", "graphics", 2117, 75),
    ("villagers", "graphics", 1552, 75),
    ("villagers", "graphics", 1560, 75),
    ("scrub", "graphics", 4474, 6),
    ("snag", "graphics", 1262, 1),
];
pub(super) const ARROW: u32 = 3799;
pub(super) const JAVELIN: u32 = 3820;
pub(super) const HOUSE: u32 = 90_000;
pub(super) const FIRE_25: u32 = 90_001;
pub(super) const FIRE_50: u32 = 90_002;
/// The DE flag's column, clear of every trial sheet.
pub(super) const FLAG: usize = 60;
const COLUMNS: usize = 64;
const NATIVE_HOUSE: &str = "de/b_synthetic_house.sld";
const NATIVE_TREE: &str = "de/n_synthetic_tree.sld";
const NATIVE_FIRE: &str = "de/particles/fire_synthetic";
/// Every DAT terrain the native ground binds, with both Farm surfaces.
const GROUND_IDS: [u16; 19] = [
    12, 100, 6, 14, 40, 1, 10, 11, 9, 0, 2, 124, 23, 22, 26, 4, 24, 7, 29,
];

fn slp(archive: &str, id: u32) -> String {
    format!("{archive}.drs:[32, 112, 108, 115]:{id}")
}

pub(super) fn grey(column: usize) -> [u8; 4] {
    let level = column as u8 + 1;
    [level, level, level, 255]
}

pub(super) fn column(id: u32) -> usize {
    SHEETS.iter().position(|sheet| sheet.2 == id).unwrap()
}

pub(super) struct Pack(Value);

impl Pack {
    fn new() -> Self {
        let mut value = manifest::fixture();
        value["pages"] = json!([]);
        value["frames"] = json!([]);
        Self(value)
    }

    /// Opens a source page. Later `sheet` calls put their frames on it.
    fn page(&mut self, name: &str) {
        let mut page = manifest::fixture()["pages"][0].clone();
        for plane in ["color", "player", "shadow"] {
            page[plane] = format!("{name}-{plane}.png").into();
        }
        self.0["pages"].as_array_mut().unwrap().push(page);
    }

    fn sheet(&mut self, source: &str, count: u32, column: usize) {
        let page = self.0["pages"].as_array().unwrap().len() - 1;
        let frames = self.0["frames"].as_array_mut().unwrap();
        for frame in 0..count {
            frames.push(json!({
                "source": source, "source_hash": "hash", "frame": frame, "page": page,
                "x": column, "y": 0, "width": 1, "height": 1, "anchor_x": 1, "anchor_y": 2
            }));
        }
    }

    /// Streamed cells: they never reach a PNG page.
    fn native(&mut self, source: &str, count: u32) {
        let frames = self.0["frames"].as_array_mut().unwrap();
        for frame in 0..count {
            let offset = frames.len();
            frames.push(json!({
                "source": source, "source_hash": "hash", "frame": frame, "page": 0,
                "x": 0, "y": 0, "width": 2, "height": 2, "anchor_x": 1, "anchor_y": 2,
                "native": {"chunk": 0, "offset": offset, "length": 1}
            }));
        }
        let bytes = frames.len();
        self.0["chunks"] =
            json!([{"file": "frames-0000.bin", "hash": "a".repeat(64), "bytes": bytes}]);
    }

    fn recipes(&mut self, buildings: Vec<Value>) {
        self.0["buildings"] = buildings.into();
        let buildings: Vec<aoe_assets::dat::BuildingComposition> =
            serde_json::from_value(self.0["buildings"].clone()).unwrap();
        let frames: Vec<aoe_assets::pack::FrameRecord> =
            serde_json::from_value(self.0["frames"].clone()).unwrap();
        self.0["building_data"] = aoe_assets::pack::recipes::encode(&buildings, &frames)
            .unwrap()
            .into();
    }
}

fn part(source: &str, frames: u16, seconds: f32) -> Value {
    json!({
        "graphic_id": 1, "slp_id": 1, "source": source, "available": true, "frames": frames,
        "angles": 1, "primary": false, "frame_rate": seconds, "replay_delay": 0.0,
        "offset": [0, 0], "misplacement": [0.0, 0.0], "layer": 20
    })
}

fn stage(percent: i16, parts: Vec<Value>) -> Value {
    json!({"damage_percent": percent, "apply_mode": 0, "parts": parts})
}

fn recipe(unit: i32, parts: Vec<Value>, damage: Vec<Value>) -> Value {
    json!({
        "unit_id": unit, "footprint": [2.0, 2.0], "foundation_terrain": -1,
        "foundation_slp": null, "foundation_size": [0, 0], "parts": parts,
        "construction": [], "states": [], "damage": damage
    })
}

/// The whole trial, undrawn sheets included, and one House with two fires.
pub(super) fn trial() -> Pack {
    let mut pack = Pack::new();
    let mut open = "";
    for (column, &(page, archive, id, frames)) in SHEETS.iter().enumerate() {
        if page != open {
            pack.page(page);
            open = page;
        }
        pack.sheet(&slp(archive, id), frames, column);
    }
    let fire = |id| vec![part(&slp("graphics", id), 2, 0.05)];
    pack.recipes(vec![recipe(
        70,
        vec![part(&slp("graphics", HOUSE), 3, 0.0)],
        vec![stage(25, fire(FIRE_25)), stage(50, fire(FIRE_50))],
    )]);
    pack
}

/// World and actor categories: native ground, tree, House, villager and one
/// native fire. Only the flag uses a PNG page, and the DE House declares no
/// fire at 50% damage, so the trial fire of that stage stays drawn.
pub(super) fn overlay() -> [Pack; 2] {
    let mut world = Pack::new();
    world.page("flag");
    world.sheet("de/b_misc_waypoint_flag_x1.sld", 90, FLAG);
    world.native(NATIVE_HOUSE, 1);
    world.native(NATIVE_TREE, 1);
    let mut terrains = Vec::new();
    for id in GROUND_IDS {
        let source = format!("de/synthetic_{id}.dds");
        world.native(&source, 1);
        terrains.push(json!({"id": id, "source": source, "size": [16, 16], "period": [10, 10]}));
    }
    world.0["terrains"] = terrains.into();
    world.recipes(vec![
        recipe(
            70,
            vec![part(NATIVE_HOUSE, 1, 0.0)],
            vec![stage(25, Vec::new()), stage(50, Vec::new())],
        ),
        recipe(349, vec![part(NATIVE_TREE, 1, 0.0)], Vec::new()),
    ]);
    let mut actors = Pack::new();
    let villager = SPRITES[Pose::VillagerStand as usize];
    actors.native(&villager.manifest_source(), villager.frames());
    actors.native(NATIVE_FIRE, 2);
    let fire = vec![part(NATIVE_FIRE, 2, 0.05)];
    actors.recipes(vec![recipe(70, Vec::new(), vec![stage(25, fire)])]);
    [world, actors]
}

fn encode(pixels: &[u8]) -> Uint8Array {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 2048, 2048);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(pixels).unwrap();
        writer.finish().unwrap();
    }
    Uint8Array::from(bytes.as_slice())
}

/// Serves the packs and records every PNG plane the loader asks for.
pub(super) struct Packs(JsValue);

impl Packs {
    pub(super) fn install(base: &Pack, overlay: Option<&[Pack; 2]>) -> Self {
        let mut manifests = serde_json::Map::new();
        let mut serve = |prefix: &str, pack: &Pack| {
            let path = format!("/asset-pack/{prefix}manifest.json");
            manifests.insert(path, pack.0.to_string().into());
        };
        serve("", base);
        if let Some([world, actors]) = overlay {
            serve("buildings/", world);
            serve("buildings/actors/", actors);
        }
        let mut pixels = vec![0; GAME_ATLAS_PAGE_BYTES];
        for (column, texel) in pixels.chunks_exact_mut(4).take(COLUMNS).enumerate() {
            texel.copy_from_slice(&grey(column));
        }
        let color = encode(&pixels);
        pixels.fill(0);
        let mask = encode(&pixels);
        let install = js_sys::Function::new_with_args(
            "manifests, color, mask",
            "const original = window.fetch, texts = JSON.parse(manifests);
             window.__aoeAssetRequests = [];
             window.fetch = async path => {
                 if (path.endsWith('manifest.json'))
                     return path in texts ? new Response(texts[path]) : new Response(null, {status:404});
                 window.__aoeAssetRequests.push(path);
                 return new Response(path.endsWith('color.png') ? color : mask);
             };
             return original;",
        );
        let manifests = Value::Object(manifests).to_string();
        Self(
            install
                .call3(
                    &JsValue::NULL,
                    &JsValue::from_str(&manifests),
                    &color,
                    &mask,
                )
                .unwrap(),
        )
    }

    /// Takes the recorded planes, sorted; repeats are kept.
    pub(super) fn fetched(&self) -> Vec<String> {
        let requests = js_sys::Function::new_no_args(
            "const taken = window.__aoeAssetRequests; window.__aoeAssetRequests = []; return taken;",
        )
        .call0(&JsValue::NULL)
        .unwrap();
        let mut planes = js_sys::Array::from(&requests)
            .iter()
            .map(|path| path.as_string().unwrap())
            .collect::<Vec<_>>();
        planes.sort_unstable();
        planes
    }
}

impl Drop for Packs {
    fn drop(&mut self) {
        js_sys::Reflect::set(&web_sys::window().unwrap(), &"fetch".into(), &self.0).unwrap();
    }
}

/// The three decoded planes of each named source page, sorted.
pub(super) fn planes(pages: &[&str]) -> Vec<String> {
    let mut planes = pages
        .iter()
        .flat_map(|page| {
            ["color", "player", "shadow"].map(|plane| format!("/asset-pack/{page}-{plane}.png"))
        })
        .collect::<Vec<_>>();
    planes.sort_unstable();
    planes
}

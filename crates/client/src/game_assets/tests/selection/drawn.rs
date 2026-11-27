//! Page start fetches only the PNG pages that something draws.
use super::*;
use aoe_assets::catalog::units::{Pose, SPRITES};
use packs::{
    ARROW, FIRE_25, FIRE_50, FLAG, HOUSE, JAVELIN, Packs, SHEETS, column, grey, overlay, planes,
    trial,
};
use std::collections::HashMap;

#[path = "packs.rs"]
mod packs;

/// Mask texel of an owned, opaque cell: base alpha in B, no player ramp on grey.
const OWNER_MASK: [u8; 4] = [0, 0, 255, 0];

/// The WebGPU source-batch upload, keeping each one-texel cell and no atlas.
#[derive(Default)]
struct Texels {
    cells: HashMap<(u32, u32, u32), [u8; 4]>,
    /// Owner mask texel of the cells that have one, under the colour cell's key.
    masks: HashMap<(u32, u32, u32), [u8; 4]>,
    scratch: Vec<u8>,
}

impl AtlasSink for Texels {
    fn configure(&mut self, _: u32) -> Result<(), String> {
        Ok(())
    }
    fn source_batches(&self) -> bool {
        true
    }
    fn write_source(
        &mut self,
        colour: &[u8],
        mask: Option<&[u8]>,
        copies: &[aoe_rendering::AtlasSourceCopy],
    ) -> Result<js_sys::Promise, String> {
        for copy in copies {
            let [x, y] = copy.source.map(usize::from);
            let from = (y * GAME_ATLAS_SIDE as usize + x) * 4;
            let texel = |pixels: &[u8]| <[u8; 4]>::try_from(&pixels[from..from + 4]).unwrap();
            let [left, top] = copy.destination.map(u32::from);
            let cell = (u32::from(copy.colour), left, top);
            self.cells.insert(cell, texel(colour));
            if let Some(page) = copy.mask {
                assert_eq!(u32::from(page), aoe_rendering::atlas_mask_page(cell.0));
                let pixels = mask.expect("a copy with a mask page has mask pixels");
                self.masks.insert(cell, texel(pixels));
            }
        }
        Ok(js_sys::Promise::resolve(&JsValue::UNDEFINED))
    }
    fn page_buffer(&mut self, _: u32) -> Result<&mut [u8], String> {
        self.scratch.clear();
        self.scratch.resize(GAME_ATLAS_PAGE_BYTES, 0);
        Ok(&mut self.scratch)
    }
    fn commit(&mut self, _: u32) -> Result<(), String> {
        Ok(())
    }
    fn finish(&mut self) -> Result<(), String> {
        Ok(())
    }
}

impl Texels {
    /// `count` static frames, each holding the texel of one sheet's column.
    fn assert_drawn(&self, what: &str, frames: &[GameFrame], count: usize, column: usize) {
        assert_eq!(frames.len(), count, "{what}");
        for frame in frames {
            assert_eq!(frame.stream, 0, "{what} is static PNG art");
            let [x, y] = [0, 1].map(|axis| frame.atlas.uv[axis] * GAME_ATLAS_SIDE as f32);
            let cell = (frame.atlas.page, x as u32, y as u32);
            assert_eq!(self.cells.get(&cell), Some(&grey(column)), "{what}");
        }
    }

    /// Every frame has an owner mask to recolour, or none of them has one.
    fn assert_owned(&self, what: &str, frames: &[GameFrame], owned: bool) {
        assert!(!frames.is_empty(), "{what}");
        for frame in frames {
            let [x, y] = [0, 1].map(|axis| frame.atlas.uv[axis] * GAME_ATLAS_SIDE as f32);
            let mask = self.masks.get(&(frame.atlas.page, x as u32, y as u32));
            assert_eq!(mask, owned.then_some(&OWNER_MASK), "{what}: owner mask");
        }
    }
}

/// The WebGL2 and Canvas upload: source order into the retained CPU atlas.
struct Retained(Vec<u8>);

impl AtlasSink for Retained {
    fn configure(&mut self, _: u32) -> Result<(), String> {
        // Streamed pairs after the eight base layers hold no page-start cell.
        self.0 = vec![0; GAME_ATLAS_BYTES];
        Ok(())
    }
    fn resident_buffer(&mut self) -> Result<Option<&mut [u8]>, String> {
        Ok(Some(&mut self.0))
    }
    fn page_buffer(&mut self, _: u32) -> Result<&mut [u8], String> {
        Err("The retained atlas is filled in place".into())
    }
    fn commit(&mut self, _: u32) -> Result<(), String> {
        Ok(())
    }
    fn finish(&mut self) -> Result<(), String> {
        Ok(())
    }
}

fn assert_only(fetched: &[String], drawn: &[String], what: &str) {
    let wasted = fetched
        .iter()
        .filter(|plane| !drawn.contains(plane))
        .collect::<Vec<_>>();
    assert!(
        wasted.is_empty(),
        "{what} decoded planes that nothing draws: {wasted:?}"
    );
    assert_eq!(fetched, drawn, "{what}: each drawn plane exactly once");
}

#[wasm_bindgen_test]
async fn a_de_overlay_fetches_only_the_png_pages_it_draws() {
    let overlay = overlay();
    let packs = Packs::install(&trial(), Some(&overlay));
    let mut texels = Texels::default();
    load(&mut texels).await.expect("WebGPU upload starts");
    let batched = packs.fetched();
    let mut retained = Retained(Vec::new());
    load(&mut retained)
        .await
        .expect("WebGL2/Canvas upload starts");
    let resident = packs.fetched();
    drop(packs);
    // Arrow and javelin flights, the one fire stage without DE art, the flag.
    let drawn = planes(&["projectiles", "fire-50", "buildings/flag"]);
    assert_only(&batched, &drawn, "WebGPU");
    assert_only(&resident, &drawn, "WebGL2/Canvas");
}

#[wasm_bindgen_test]
async fn a_de_overlay_keeps_its_native_cells_and_the_trial_cells_it_still_draws() {
    let overlay = overlay();
    let packs = Packs::install(&trial(), Some(&overlay));
    let mut texels = Texels::default();
    let loaded = load(&mut texels).await;
    drop(packs);
    let (art, _, native) = loaded.expect("DE-like overlay starts");
    assert!(native.is_some(), "native cells stream after page start");
    let actor = |pose: Pose| &art.actors[pose as usize];
    texels.assert_drawn("arrow", actor(Pose::Arrow), 5, column(ARROW));
    texels.assert_drawn("javelin", actor(Pose::Javelin), 5, column(JAVELIN));
    texels.assert_drawn("flag", &art.scenery.rally_flags, 10, FLAG);
    // Actors and building art are recoloured per owner; the flag is shared art.
    texels.assert_owned("arrow", actor(Pose::Arrow), true);
    texels.assert_owned("javelin", actor(Pose::Javelin), true);
    texels.assert_owned("flag", &art.scenery.rally_flags, false);
    let villager = SPRITES[Pose::VillagerStand as usize];
    assert_eq!(
        actor(Pose::VillagerStand).len(),
        villager.selected_frames(),
        "villagers are native actors"
    );
    assert!(actor(Pose::VillagerStand).iter().all(|f| f.stream != 0));
    let house = art.compositions.iter().find(|b| b.unit_id == 70).unwrap();
    assert_ne!(house.parts[0].frame.stream, 0, "native body");
    let [early, late] = house.damage.as_slice() else {
        panic!("both damage stages survive the overlay");
    };
    assert_eq!(early.parts.len(), 1, "native fire replaces the trial fire");
    assert!(early.parts[0].animation.iter().all(|f| f.stream != 0));
    assert_eq!(late.parts.len(), 1, "a stage without DE fire keeps its own");
    texels.assert_drawn("fire", &late.parts[0].animation, 2, column(FIRE_50));
    texels.assert_owned("fire", &late.parts[0].animation, true);
    assert!(art.environment.iter().any(|b| b.unit_id == 349));
    for (slot, frames) in art.terrain.iter().enumerate() {
        assert_eq!(frames.len(), 1, "native ground {slot}");
        assert_ne!(frames[0].stream, 0, "native ground {slot}");
    }
}

const TRIAL_PAGES: [&str; 6] = [
    "ground",
    "objects",
    "projectiles",
    "house",
    "fire-25",
    "fire-50",
];

#[wasm_bindgen_test]
async fn a_trial_pack_fetches_only_the_png_pages_it_draws() {
    let packs = Packs::install(&trial(), None);
    let mut texels = Texels::default();
    load(&mut texels).await.expect("trial-like pack starts");
    let fetched = packs.fetched();
    drop(packs);
    assert_only(&fetched, &planes(&TRIAL_PAGES), "trial");
}

#[wasm_bindgen_test]
async fn a_trial_pack_keeps_every_sheet_it_really_draws() {
    let packs = Packs::install(&trial(), None);
    let mut texels = Texels::default();
    let loaded = load(&mut texels).await;
    let fetched = packs.fetched();
    drop(packs);
    let (art, _, native) = loaded.expect("trial-like pack starts");
    assert!(native.is_none(), "the trial streams nothing");
    for plane in planes(&TRIAL_PAGES) {
        assert!(fetched.contains(&plane), "{plane}");
    }
    let sheet = |frames: &[GameFrame], id: u32| {
        let row = column(id);
        let what = format!("SLP {id}");
        texels.assert_drawn(&what, frames, SHEETS[row].3 as usize, row);
    };
    for (frames, id) in art.terrain.iter().zip([
        15008, 15007, 15000, 15010, 15033, 15002, 15011, 15006, 15009, 15001, 15017, 15026,
    ]) {
        sheet(frames, id);
    }
    for ((frames, _), id) in art.water_depth.iter().zip([15016, 15015, 15024, 15014]) {
        sheet(frames, id);
    }
    for (frames, id) in art.resources.iter().zip([2560, 4652, 4479, 4482]) {
        sheet(frames, id);
    }
    sheet(&art.tree_shadows, 2296);
    sheet(&art.tree_families[0], 4654);
    sheet(&art.tree_families[1], 4653);
    sheet(&art.scenery.cliff_faces, 226);
    sheet(&art.scenery.rocks, 1034);
    sheet(&art.scenery.stumps, 1035);
    sheet(&art.scenery.logs, 1252);
    assert!(
        art.scenery.rally_flags.is_empty(),
        "the trial archives omit the flag; the shared fallback draws it"
    );
    for (pose, id) in [(Pose::Arrow, ARROW), (Pose::Javelin, JAVELIN)] {
        texels.assert_drawn("flight", &art.actors[pose as usize], 5, column(id));
    }
    let house = art.compositions.iter().find(|b| b.unit_id == 70).unwrap();
    sheet(&house.parts[0].animation, HOUSE);
    for (stage, id) in house.damage.iter().zip([FIRE_25, FIRE_50]) {
        sheet(&stage.parts[0].animation, id);
    }
}

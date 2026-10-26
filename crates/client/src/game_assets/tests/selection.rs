use super::*;
use std::collections::BTreeMap;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn bounded_role_ranges_preserve_reference_lookup_absence_and_replacement() {
    let mut ranges: RoleRanges = std::array::from_fn(|_| None);
    let mut reference = BTreeMap::new();
    let sources = REQUIRED_RENDER_SOURCES
        .iter()
        .chain(&OPTIONAL_RESOURCE_SOURCES)
        .chain(&OPTIONAL_TERRAIN_SOURCES);
    for source in sources.clone() {
        assert!((source.role as usize) < ROLE_SLOTS);
        assert_eq!(ranges[source.role as usize], None);
        for range in [0..source.frames as usize, 2..source.frames as usize + 2] {
            reference.insert(source.role, range.clone());
            ranges[source.role as usize] = Some(range);
            assert_eq!(
                ranges[source.role as usize].as_ref(),
                reference.get(&source.role)
            );
        }
    }
    for role in sources.map(|source| source.role).chain([AssetRole::Rock]) {
        assert_eq!(
            ranges.get(role as usize).and_then(Option::as_ref),
            reference.get(&role)
        );
    }
    assert!(ranges[AssetRole::Rock as usize].is_some());
    assert_eq!(ROLE_SLOTS, 41);
    assert!(ranges[AssetRole::TreeConifer as usize].is_some());
    assert!(ranges[AssetRole::TreePalm as usize].is_some());
}

#[wasm_bindgen_test]
fn optional_source_absence_is_distinct_from_corrupt_present_frames() {
    let manifest: Manifest = serde_json::from_value(manifest::fixture()).unwrap();
    assert!(!source_present(&manifest, "absent"));
    assert_eq!(source_frames(&manifest, "absent", 2), None);
    assert!(source_present(&manifest, "fixture"));
    assert_eq!(source_frames(&manifest, "fixture", 2), None);
}

#[wasm_bindgen_test]
fn source_indices_preserve_frame_order_and_reject_missing_or_duplicate_frames() {
    let mut value = manifest::fixture();
    let first = value["frames"][0].clone();
    value["frames"] = serde_json::json!([first.clone(), first.clone(), first.clone()]);
    for (index, frame) in [2, 0, 1].into_iter().enumerate() {
        value["frames"][index]["frame"] = frame.into();
    }
    let complete: Manifest = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(source_frames(&complete, "fixture", 3), Some(vec![1, 2, 0]));
    assert_eq!(source_frames(&complete, "absent", 3), None);
    assert_eq!(source_frames(&complete, "fixture", 4), None);
    value["frames"][2]["frame"] = 0.into();
    let duplicate: Manifest = serde_json::from_value(value).unwrap();
    assert_eq!(source_frames(&duplicate, "fixture", 3), None);
}

#[wasm_bindgen_test]
fn optional_tree_families_require_complete_unique_native_prefixes() {
    for role in [
        AssetRole::TreeConifer,
        AssetRole::TreePalm,
        AssetRole::Shrub,
        AssetRole::DeadwoodStump,
        AssetRole::DeadwoodLog,
        AssetRole::DeadwoodTree,
    ] {
        let selection = OPTIONAL_RESOURCE_SOURCES
            .iter()
            .find(|source| source.role == role)
            .unwrap();
        let identity = selection.manifest_source();
        let mut value = manifest::fixture();
        let template = value["frames"][0].clone();
        let absent: Manifest = serde_json::from_value(value.clone()).unwrap();
        assert!(!source_present(&absent, &identity));
        assert!(source_frames(&absent, &identity, selection.frames).is_none());
        let mut entries = Vec::new();
        for number in (0..selection.frames).rev() {
            let mut frame = template.clone();
            frame["source"] = identity.clone().into();
            frame["frame"] = number.into();
            entries.push(frame);
        }
        value["frames"] = entries.into();
        let complete: Manifest = serde_json::from_value(value.clone()).unwrap();
        assert!(source_present(&complete, &identity));
        let expected = (0..selection.frames as usize).rev().collect::<Vec<_>>();
        assert_eq!(
            source_frames(&complete, &identity, selection.frames),
            Some(expected)
        );
        // Presence is never downgraded to absence by an incomplete/duplicate prefix.
        let mut missing_value = value.clone();
        missing_value["frames"][0]["frame"] = selection.frames.into();
        let missing: Manifest = serde_json::from_value(missing_value).unwrap();
        assert!(source_present(&missing, &identity));
        assert!(source_frames(&missing, &identity, selection.frames).is_none());
        let first = value["frames"][0].clone();
        value["frames"].as_array_mut().unwrap().push(first);
        let duplicate: Manifest = serde_json::from_value(value).unwrap();
        assert!(source_present(&duplicate, &identity));
        assert!(source_frames(&duplicate, &identity, selection.frames).is_none());
    }
}

#[wasm_bindgen_test]
fn native_forest_source_loads_only_ten_accents_from_the_full_hundred() {
    let mut value = manifest::fixture();
    let frame = value["frames"][0].clone();
    value["frames"] = serde_json::Value::Array(vec![frame; 100]);
    let selection = OPTIONAL_TERRAIN_SOURCES[0];
    let source = selection.manifest_source();
    for index in 0..100 {
        value["frames"][index]["frame"] = (index as u32).into();
        value["frames"][index]["source"] = source.clone().into();
    }
    let manifest: Manifest = serde_json::from_value(value).unwrap();
    assert_eq!(
        source_frames(&manifest, &source, selection.frames),
        Some((0..10).collect())
    );
}

#[wasm_bindgen_test]
fn startup_index_sort_matches_std_at_catalogue_bound() {
    let bound = REQUIRED_RENDER_SOURCES
        .iter()
        .chain(OPTIONAL_RESOURCE_SOURCES.iter())
        .chain(OPTIONAL_TERRAIN_SOURCES.iter())
        .map(|selection| selection.frames as usize)
        .sum::<usize>();
    for size in [0, 1, bound.saturating_sub(2), bound] {
        let mut shuffled = (0..size).collect::<Vec<_>>();
        let mut seed = 0x2a97_1845_u32;
        for index in (1..size).rev() {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            shuffled.swap(index, seed as usize % (index + 1));
        }
        for input in [
            (0..size).collect::<Vec<_>>(),
            (0..size).rev().collect(),
            vec![17; size],
            shuffled,
        ] {
            let mut actual = input.clone();
            let mut expected = input;
            expected.sort_unstable();
            let mut comparisons = 0_usize;
            sort_indices(&mut actual, &mut |left, right| {
                comparisons += 1;
                left.cmp(right)
            });
            assert_eq!(actual, expected);
            assert!(comparisons <= size * size.saturating_sub(1) / 2);
        }
    }
    let mut value = manifest::fixture();
    let frame = value["frames"][0].clone();
    value["frames"] = serde_json::Value::Array(vec![frame; bound + 1]);
    let oversized: Manifest = serde_json::from_value(value).unwrap();
    assert_eq!(source_frames(&oversized, "fixture", 3), None);
}

// Exercise the real asynchronous loader with generated PNGs, never original art.
pub(super) struct FetchFixture(JsValue);

impl FetchFixture {
    pub(super) fn install(value: &serde_json::Value) -> Self {
        Self::install_with_player(value, false)
    }

    pub(super) fn install_with_player(value: &serde_json::Value, player: bool) -> Self {
        let mut pixels = vec![0; GAME_ATLAS_PAGE_BYTES];
        pixels[..4].copy_from_slice(&[80, 90, 100, 255]);
        pixels[8..12].copy_from_slice(&[120, 120, 120, if player { 128 } else { 255 }]);
        let encode = |pixels: &[u8]| {
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
        };
        let color = encode(&pixels);
        pixels.fill(0);
        if player {
            pixels[8..12].copy_from_slice(&[7, 0, 0, 119]);
        }
        let mask = encode(&pixels);
        let install = js_sys::Function::new_with_args(
            "manifest, color, mask",
            "const original = window.fetch;
             window.fetch = async path => path.includes('/buildings/') ? new Response(null, {status:404}) : new Response(
                 path.endsWith('manifest.json') ? manifest :
                 path.endsWith('color.png') ? color : mask);
             return original;",
        );
        Self(
            install
                .call3(
                    &JsValue::NULL,
                    &JsValue::from_str(&value.to_string()),
                    &color,
                    &mask,
                )
                .unwrap(),
        )
    }
}

impl Drop for FetchFixture {
    fn drop(&mut self) {
        js_sys::Reflect::set(&web_sys::window().unwrap(), &"fetch".into(), &self.0).unwrap();
    }
}

pub(super) fn pack_fixture(
    sources: impl IntoIterator<Item = (String, u32, bool)>,
) -> serde_json::Value {
    let mut value = manifest::fixture();
    let template = value["frames"][0].clone();
    value["pages"][0]["color"] = "color.png".into();
    let mut frames = Vec::new();
    for (source, count, terrain) in sources {
        for number in 0..count {
            let mut frame = template.clone();
            frame["source"] = source.clone().into();
            frame["frame"] = number.into();
            frame["x"] = (if terrain { 0 } else { 2 }).into();
            frame["width"] = 1.into();
            frame["height"] = 1.into();
            frames.push(frame);
        }
    }
    value["frames"] = frames.into();
    value
}

#[wasm_bindgen_test]
async fn minimal_pack_starts_without_mountain_sources_and_present_art_is_loaded() {
    // Independent fixture contract: cavalry plus the five base ground sheets.
    let minimal = || {
        [
            ("graphics.drs", 3008, 50),
            ("graphics.drs", 3004, 50),
            ("terrain.drs", 15008, 100),
            ("terrain.drs", 15007, 100),
            ("terrain.drs", 15000, 100),
            ("terrain.drs", 15010, 100),
            ("terrain.drs", 15002, 100),
        ]
        .map(|(archive, id, count)| {
            (
                format!("{archive}:[32, 112, 108, 115]:{id}"),
                count,
                archive == "terrain.drs",
            )
        })
    };
    for mountain in [false, true] {
        let sources = minimal().into_iter().chain(
            [15033, 15026]
                .into_iter()
                .filter(|_| mountain)
                .map(|id| (format!("terrain.drs:[32, 112, 108, 115]:{id}"), 100, true)),
        );
        let value = pack_fixture(sources);
        let fetch = FetchFixture::install(&value);
        let loaded = load_collected().await;
        drop(fetch);
        let (art, pixels, _) = loaded.expect("minimal pack must initialise");
        assert_eq!(art.walking.len(), 50);
        assert_eq!(art.terrain[2].len(), 100, "base fallback remains available");
        for slot in [4, 11] {
            assert_eq!(art.terrain[slot].len(), if mountain { 100 } else { 0 });
            if mountain {
                let frame = art.terrain[slot][0];
                let at = frame.atlas.page as usize * GAME_ATLAS_PAGE_BYTES
                    + ((frame.atlas.uv[1] * 2048.0) as usize * 2048
                        + (frame.atlas.uv[0] * 2048.0) as usize)
                        * 4;
                assert_eq!(pixels[at..at + 4], [80, 90, 100, 255]);
            }
        }
    }
    for id in [15033, 15026] {
        let source = format!("terrain.drs:[32, 112, 108, 115]:{id}");
        let value = pack_fixture(minimal().into_iter().chain([(source.clone(), 99, true)]));
        let fetch = FetchFixture::install(&value);
        let loaded = load_collected().await;
        drop(fetch);
        assert!(
            loaded.err().unwrap().as_string().unwrap().contains(&source),
            "present incomplete mountain art must remain an error"
        );
    }
}

#[wasm_bindgen_test]
async fn full_catalog_packs_1474_terrain_frames_and_782_objects_in_their_domains() {
    let sources = REQUIRED_RENDER_SOURCES
        .iter()
        .chain(&OPTIONAL_RESOURCE_SOURCES)
        .chain(&OPTIONAL_TERRAIN_SOURCES)
        .map(|source| {
            (
                source.manifest_source(),
                source.frames,
                source.terrain_topology.is_some(),
            )
        });
    let value = pack_fixture(sources);
    let fetch = FetchFixture::install(&value);
    let loaded = load_collected().await;
    drop(fetch);
    let (_, pixels, _) = loaded.expect("full catalog must initialise");
    let (terrain, objects) = pixels.split_at(2 * GAME_ATLAS_PAGE_BYTES);
    let count = |pixels: &[u8], color: [u8; 4]| {
        pixels
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| **pixel == color)
            .count()
    };
    assert_eq!(count(terrain, [80, 90, 100, 255]), 1474);
    assert_eq!(
        count(objects, [80, 90, 100, 255]),
        0,
        "marsh belongs on terrain pages"
    );
    // Castle Light Cavalry keeps its five standing and ten walking semantic
    // entries while sharing their existing scout texels: still 782 stored cells.
    assert_eq!(count(objects, [120, 120, 120, 255]), 782);
    assert_eq!(count(terrain, [120, 120, 120, 255]), 0);
}

/// Records what the loader hands over, never keeping a page.
#[derive(Default)]
struct PageLog {
    pages: Vec<(u32, usize)>,
    scratch: Vec<u8>,
    finished: usize,
}

impl AtlasSink for PageLog {
    fn page_buffer(&mut self, _: u32) -> Result<&mut [u8], String> {
        self.scratch.clear();
        self.scratch.resize(GAME_ATLAS_PAGE_BYTES, 0);
        Ok(&mut self.scratch)
    }
    fn commit(&mut self, index: u32) -> Result<(), String> {
        self.pages.push((index, self.scratch.len()));
        Ok(())
    }
    fn finish(&mut self) -> Result<(), String> {
        self.finished += 1;
        Ok(())
    }
}

#[wasm_bindgen_test]
async fn the_atlas_streams_one_page_at_a_time_and_is_never_resident_whole() {
    let value = pack_fixture(
        [
            ("graphics.drs", 3008, 50, false),
            ("graphics.drs", 3004, 50, false),
            ("terrain.drs", 15008, 100, true),
            ("terrain.drs", 15007, 100, true),
            ("terrain.drs", 15000, 100, true),
            ("terrain.drs", 15010, 100, true),
            ("terrain.drs", 15002, 100, true),
        ]
        .into_iter()
        .map(|(archive, id, count, terrain)| {
            (
                format!("{archive}:[32, 112, 108, 115]:{id}"),
                count,
                terrain,
            )
        }),
    );
    let fetch = FetchFixture::install(&value);
    let mut log = PageLog::default();
    let loaded = load(&mut log).await;
    drop(fetch);
    loaded.expect("minimal pack must initialise");
    // Every page arrives once, in order, as one page buffer; finish follows.
    let expected = (0..GAME_ATLAS_PAGES)
        .map(|page| (page, GAME_ATLAS_PAGE_BYTES))
        .collect::<Vec<_>>();
    assert_eq!(log.pages, expected);
    assert_eq!(log.finished, 1);
    assert!(log.pages.iter().all(|(_, bytes)| *bytes < GAME_ATLAS_BYTES));
}

//! Far-view thumbnails use the same compiled DAT recipes and native pixel codec.
use super::*;

#[wasm_bindgen]
pub fn strategic_building_parts(bytes: &[u8]) -> Result<Vec<f64>, JsValue> {
    let manifest = manifest::decode(bytes)?;
    let recipes = recipes::decode(&manifest.building_data, &manifest.frames).map_err(error)?;
    let mut output = Vec::new();
    for building in recipes {
        let parts: Vec<_> = building
            .parts
            .iter()
            .filter_map(|p| p.record.map(|record| (record, p)))
            .collect();
        output.extend([f64::from(building.unit_id), parts.len() as f64]);
        for (record, part) in parts {
            let [ne, se] = part.misplacement.map(f64::from);
            output.extend([
                record as f64,
                f64::from(part.offset[0]) + (ne + se) * 48.0,
                f64::from(part.offset[1]) + (se - ne) * 24.0,
            ]);
        }
    }
    Ok(output)
}

#[wasm_bindgen]
pub fn strategic_frame(
    bytes: &[u8],
    width: u16,
    height: u16,
    colour: u32,
) -> Result<Vec<u8>, JsValue> {
    let [mut pixels, mask, shadow] =
        compressed::decode_frame(bytes, [width, height]).map_err(error)?;
    let team = [(colour >> 16) as u8, (colour >> 8) as u8, colour as u8];
    for (i, pixel) in pixels.chunks_exact_mut(4).enumerate() {
        let at = i * 4;
        let base = [pixel[0], pixel[1], pixel[2], pixel[3]];
        pixel.copy_from_slice(&aoe_assets::pack::blend_player(
            base,
            mask[at],
            mask[at + 3],
            team,
        ));
        if pixel[3] == 0 {
            pixel.copy_from_slice(&shadow[at..at + 4]);
        }
    }
    Ok(pixels)
}

fn miniature_records(manifest: &Manifest) -> [Option<usize>; aoe_protocol::UnitKind::ALL.len()] {
    use crate::unit_visuals::idle_pose;
    use aoe_assets::catalog::units::SPRITES;
    aoe_protocol::UnitKind::ALL.map(|kind| {
        let source = SPRITES[idle_pose(kind)? as usize];
        let cell = source.source_frame(source.rows() / 2, 0);
        manifest
            .frames
            .sources()
            .cell(&source.manifest_source(), cell as u32)
    })
}

#[wasm_bindgen(
    inline_js = "export function unit_miniatures(rows) { window.aoeNavigation?.strategic?.unitMiniatures(rows); }"
)]
extern "C" {
    fn unit_miniatures(rows: js_sys::Array);
}

/// Publish one frame descriptor per unit kind from the already loaded pack.
/// The browser fetches pixels only for kinds that actually reach the map.
pub(in super::super) fn publish_miniatures(manifest: &Manifest) {
    use js_sys::Array;
    let rows = Array::new();
    for record in miniature_records(manifest) {
        let Some(frame) = record.map(|index| &manifest.frames[index]) else {
            rows.push(&JsValue::NULL);
            continue;
        };
        let row = Array::new();
        for n in [frame.width, frame.height, frame.x, frame.y] {
            row.push(&JsValue::from(n));
        }
        if let Some(native) = frame.native {
            let Some(chunk) = manifest.chunks.get(usize::from(native.chunk)) else {
                rows.push(&JsValue::NULL);
                continue;
            };
            row.push(&JsValue::from_str(&chunk.file));
            row.push(&JsValue::from(native.offset));
            row.push(&JsValue::from(native.length));
        } else {
            let Some(page) = manifest.pages.get(usize::from(frame.page)) else {
                rows.push(&JsValue::NULL);
                continue;
            };
            row.push(&JsValue::from_str(&page.color));
            row.push(&JsValue::from_str(&page.player));
        }
        rows.push(&row);
    }
    unit_miniatures(rows);
}

#[cfg(test)]
#[path = "tests/far.rs"]
mod tests;

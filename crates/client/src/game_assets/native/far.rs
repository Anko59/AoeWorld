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

#[cfg(test)]
#[path = "tests/far.rs"]
mod tests;

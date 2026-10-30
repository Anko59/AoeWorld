use super::{MAX_FRAMES, MAX_PAGES, Manifest, PAGE, compressed, frame_chunks};
use crate::{Error, invalid};
use std::{collections::BTreeSet, fs, io::Cursor, path::Path};

pub fn verify(pack: &Path) -> Result<Manifest, Error> {
    let manifest_path = pack.join("manifest.json");
    if fs::metadata(&manifest_path)?.len() > 32 * 1024 * 1024 {
        return Err(invalid("manifest", 0, "manifest exceeds 32 MiB"));
    }
    let manifest = parse_manifest(&fs::read(manifest_path)?)?;
    for page in &manifest.pages {
        for (name, expected) in [
            (&page.color, &page.color_hash),
            (&page.player, &page.player_hash),
            (&page.shadow, &page.shadow_hash),
            (&page.outline, &page.outline_hash),
        ] {
            let path = pack.join(name);
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() || metadata.len() > 32 * 1024 * 1024 {
                return Err(invalid("manifest", 0, "invalid PNG path or size"));
            }
            let data = fs::read(path)?;
            if blake3::hash(&data).to_hex().as_str() != expected {
                return Err(invalid("manifest", 0, format!("hash mismatch: {name}")));
            }
            let mut decoder = png::Decoder::new(Cursor::new(&data));
            decoder.set_transformations(png::Transformations::IDENTITY);
            let mut reader = decoder.read_info()?;
            if reader.info().width != u32::from(page.width)
                || reader.info().height != u32::from(page.height)
                || reader.info().color_type != png::ColorType::Rgba
                || reader.info().bit_depth != png::BitDepth::Eight
            {
                return Err(invalid("manifest", 0, "PNG dimensions or format mismatch"));
            }
            let mut pixels = vec![0; reader.output_buffer_size()];
            let details = reader.next_frame(&mut pixels)?;
            if details.width != u32::from(page.width)
                || details.height != u32::from(page.height)
                || details.color_type != png::ColorType::Rgba
                || details.bit_depth != png::BitDepth::Eight
            {
                return Err(invalid("manifest", 0, "PNG dimensions or format mismatch"));
            }
        }
    }
    verify_chunks(pack, &manifest)?;
    Ok(manifest)
}

fn verify_chunks(pack: &Path, manifest: &Manifest) -> Result<(), Error> {
    let mut by_chunk = vec![Vec::new(); manifest.chunks.len()];
    for frame in &manifest.frames {
        if let Some(location) = frame.native {
            by_chunk[usize::from(location.chunk)].push((location, [frame.width, frame.height]));
        }
    }
    for (chunk, references) in manifest.chunks.iter().zip(by_chunk) {
        let path = pack.join(&chunk.file);
        let metadata = fs::symlink_metadata(&path)?;
        if !metadata.is_file() || metadata.len() != u64::from(chunk.bytes) {
            return Err(invalid(
                "manifest",
                0,
                "invalid native chunk file or length",
            ));
        }
        let bytes = fs::read(path)?;
        chunk.verify_payload(&bytes)?;
        let mut seen = BTreeSet::new();
        for (location, size) in references {
            if seen.insert((location, size)) {
                compressed::decode_frame(location.slice(&bytes)?, size)?;
            }
        }
    }
    Ok(())
}

pub fn parse_manifest(bytes: &[u8]) -> Result<Manifest, Error> {
    if bytes.len() > 32 * 1024 * 1024 {
        return Err(invalid("manifest", 0, "manifest exceeds 32 MiB"));
    }
    let manifest: Manifest = serde_json::from_slice(bytes)?;
    if manifest.version != 1
        || manifest.pages.is_empty()
        || manifest.pages.len() > MAX_PAGES
        || manifest.frames.len() > MAX_FRAMES
    {
        return Err(invalid("manifest", 0, "unsupported version or counts"));
    }
    if !valid_hash(&manifest.input_hash) {
        return Err(invalid("manifest", 0, "invalid input hash"));
    }
    validate_chunks(&manifest)?;
    for page in &manifest.pages {
        if page.width != page.height || !matches!(page.width as usize, 1_024 | PAGE) {
            return Err(invalid("manifest", 0, "atlas page dimensions"));
        }
        for name in [&page.color, &page.player, &page.shadow, &page.outline] {
            if Path::new(name).components().count() != 1 {
                return Err(invalid("manifest", 0, "invalid page path"));
            }
        }
    }
    let mut ids = BTreeSet::new();
    for frame in &manifest.frames {
        if !valid_hash(&frame.source_hash) {
            return Err(invalid("manifest", 0, "invalid source hash"));
        }
        if !ids.insert((&frame.source, frame.frame)) {
            return Err(invalid("manifest", 0, "duplicate frame"));
        }
        if let Some(location) = frame.native {
            let chunk = manifest
                .chunks
                .get(usize::from(location.chunk))
                .ok_or_else(|| invalid("manifest", 0, "invalid native chunk reference"))?;
            if frame.page != 0
                || frame.x != 0
                || frame.y != 0
                || frame.width == 0
                || frame.height == 0
                || frame.width > crate::catalog::packing::MAX_FRAME_EXTENT
                || frame.height > crate::catalog::packing::MAX_FRAME_EXTENT
                || location.length == 0
                || location.length as usize > compressed::MAX_ENCODED_FRAME_BYTES
                || u64::from(location.offset) + u64::from(location.length) > u64::from(chunk.bytes)
            {
                return Err(invalid("manifest", 0, "invalid native frame bounds"));
            }
            continue;
        }
        let Some(page) = manifest.pages.get(frame.page as usize) else {
            return Err(invalid("manifest", 0, "invalid page reference"));
        };
        if frame.width == 0
            || frame.height == 0
            || u32::from(frame.x) + u32::from(frame.width) > u32::from(page.width)
            || u32::from(frame.y) + u32::from(frame.height) > u32::from(page.height)
        {
            return Err(invalid("manifest", 0, "frame outside atlas"));
        }
    }
    let mut unit_ids = BTreeSet::new();
    for building in &manifest.buildings {
        if building.foundation_slp.is_some()
            && (building.foundation_size.contains(&0)
                || u32::from(building.foundation_size[0]) * u32::from(building.foundation_size[1])
                    > 2560)
        {
            return Err(invalid("manifest", 0, "invalid DAT foundation topology"));
        }
        if manifest.buildings.len() > crate::catalog::GAMEPLAY_BUILDING_IDS.len()
            || !unit_ids.insert(building.unit_id)
            || building
                .footprint
                .iter()
                .any(|v| !v.is_finite() || *v <= 0.0 || *v > 128.0)
            || building.parts.len() > 256
            || building.construction.len() > 256
            || building.states.len() > 8
            || building.states.iter().enumerate().any(|(i, s)| {
                s.role > 7
                    || s.parts.len() > 256
                    || building.states[..i].iter().any(|p| p.role == s.role)
            })
            || building.damage.len() > 7
            || building.damage.iter().any(|d| {
                !(0..=100).contains(&d.damage_percent) || d.apply_mode > 2 || d.parts.len() > 256
            })
        {
            return Err(invalid("manifest", 0, "invalid building recipe"));
        }
        for part in building
            .parts
            .iter()
            .chain(&building.construction)
            .chain(building.states.iter().flat_map(|s| &s.parts))
            .chain(building.damage.iter().flat_map(|d| &d.parts))
        {
            if part.frames == 0
                || part.angles == 0
                || u32::from(part.frames) * u32::from(part.angles) > 16384
                || !part.frame_rate.is_finite()
                || part.frame_rate < 0.0
                || !part.replay_delay.is_finite()
                || part.replay_delay < 0.0
            {
                return Err(invalid("manifest", 0, "invalid building animation"));
            }
            if part.offset.iter().any(|v| v.unsigned_abs() > 32768)
                || part
                    .misplacement
                    .iter()
                    .any(|v| !v.is_finite() || v.abs() > 128.0)
                || part.available
                    != manifest
                        .frames
                        .iter()
                        .any(|f| f.source == part.source && f.frame == 0)
            {
                return Err(invalid("manifest", 0, "invalid building part or inventory"));
            }
        }
    }
    if manifest.building_data != super::recipes::encode(&manifest.buildings, &manifest.frames)? {
        return Err(invalid("manifest", 0, "compiled building recipe mismatch"));
    }
    Ok(manifest)
}

fn validate_chunks(manifest: &Manifest) -> Result<(), Error> {
    if manifest.chunks.len() > frame_chunks::MAX_CHUNKS {
        return Err(invalid("manifest", 0, "too many native chunks"));
    }
    let mut names = BTreeSet::new();
    let mut total = 0_u64;
    for chunk in &manifest.chunks {
        if !matches!(
            Path::new(&chunk.file).components().next(),
            Some(std::path::Component::Normal(_))
        ) || Path::new(&chunk.file).components().count() != 1
            || chunk.file.contains('\\')
            || !names.insert(&chunk.file)
            || !valid_hash(&chunk.hash)
            || chunk.bytes == 0
            || chunk.bytes as usize > compressed::MAX_ENCODED_FRAME_BYTES
        {
            return Err(invalid("manifest", 0, "invalid native chunk metadata"));
        }
        total += u64::from(chunk.bytes);
    }
    if total > frame_chunks::MAX_TOTAL_BYTES as u64 {
        return Err(invalid(
            "manifest",
            0,
            "native chunks exceed total byte limit",
        ));
    }
    Ok(())
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests;

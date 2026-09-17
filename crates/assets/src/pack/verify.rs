use super::{MAX_FRAMES, MAX_PAGES, Manifest, PAGE};
use crate::{Error, invalid};
use std::{collections::BTreeSet, fs, io::Cursor, path::Path};

pub fn verify(pack: &Path) -> Result<Manifest, Error> {
    let manifest_path = pack.join("manifest.json");
    if fs::metadata(&manifest_path)?.len() > 16 * 1024 * 1024 {
        return Err(invalid("manifest", 0, "manifest exceeds 16 MiB"));
    }
    let manifest: Manifest = serde_json::from_slice(&fs::read(manifest_path)?)?;
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
    for page in &manifest.pages {
        if page.width as usize != PAGE || page.height as usize != PAGE {
            return Err(invalid("manifest", 0, "atlas page dimensions"));
        }
        for (name, expected) in [
            (&page.color, &page.color_hash),
            (&page.player, &page.player_hash),
            (&page.shadow, &page.shadow_hash),
            (&page.outline, &page.outline_hash),
        ] {
            if Path::new(name).components().count() != 1 {
                return Err(invalid("manifest", 0, "invalid page path"));
            }
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
            if reader.info().width != PAGE as u32
                || reader.info().height != PAGE as u32
                || reader.info().color_type != png::ColorType::Rgba
                || reader.info().bit_depth != png::BitDepth::Eight
            {
                return Err(invalid("manifest", 0, "PNG dimensions or format mismatch"));
            }
            let mut pixels = vec![0; reader.output_buffer_size()];
            let details = reader.next_frame(&mut pixels)?;
            if details.width != PAGE as u32
                || details.height != PAGE as u32
                || details.color_type != png::ColorType::Rgba
                || details.bit_depth != png::BitDepth::Eight
            {
                return Err(invalid("manifest", 0, "PNG dimensions or format mismatch"));
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
    Ok(manifest)
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

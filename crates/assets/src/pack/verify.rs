use super::{MAX_FRAMES, MAX_PAGES, Manifest, PAGE};
use crate::{Error, invalid};
use std::{collections::BTreeSet, fs, io::Cursor, path::Path};

pub fn verify(pack: &Path) -> Result<Manifest, Error> {
    let manifest_path = pack.join("manifest.json");
    if fs::metadata(&manifest_path)?.len() > 16 * 1024 * 1024 {
        return Err(invalid("manifest", 0, "manifest exceeds 16 MiB"));
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
    Ok(manifest)
}

pub fn parse_manifest(bytes: &[u8]) -> Result<Manifest, Error> {
    if bytes.len() > 16 * 1024 * 1024 {
        return Err(invalid("manifest", 0, "manifest exceeds 16 MiB"));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_parser_checks_counts_paths_and_frame_bounds_before_file_access() {
        let hash = "a".repeat(64);
        let mut value = serde_json::json!({
            "version":1,"converter":"test","input_hash":hash,
            "pages":[{
                "color":"color.png","color_hash":hash,
                "player":"player.png","player_hash":hash,
                "shadow":"shadow.png","shadow_hash":hash,
                "outline":"outline.png","outline_hash":hash,
                "width":1024,"height":1024
            }],
            "frames":[{
                "source":"fixture","source_hash":hash,"frame":0,"page":0,
                "x":0,"y":0,"width":1,"height":1,"anchor_x":0,"anchor_y":0
            }]
        });
        let parse =
            |value: &serde_json::Value| parse_manifest(&serde_json::to_vec(value).expect("JSON"));
        assert_eq!(parse(&value).expect("valid manifest").frames.len(), 1);
        value["frames"][0]["x"] = 1024.into();
        assert!(parse(&value).is_err());
        value["frames"][0]["x"] = 0.into();
        value["pages"][0]["color"] = "../escape.png".into();
        assert!(parse(&value).is_err());
        value["pages"][0]["color"] = "color.png".into();
        let duplicate = value["frames"][0].clone();
        value["frames"]
            .as_array_mut()
            .expect("frames")
            .push(duplicate);
        assert!(parse(&value).is_err());
        value["version"] = 2.into();
        assert!(parse(&value).is_err());
        assert!(parse_manifest(&vec![0u8; 16 * 1024 * 1024 + 1]).is_err());
    }
}

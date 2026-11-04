//! Native DE interface sources; original pixels stay in the private pack.
use super::*;
use std::io::Cursor;

#[derive(serde::Serialize)]
pub(super) enum Role {
    Unit(i32),
    Command(&'static str),
    Resource(&'static str),
    Panel(&'static str),
}
pub(super) struct Source {
    pub(super) id: String,
    pub(super) path: PathBuf,
    pub(super) hash: String,
    roles: Vec<Role>,
}

fn add(sources: &mut BTreeMap<String, Source>, id: String, path: PathBuf, role: Role) {
    sources
        .entry(id.clone())
        .or_insert_with(|| Source {
            id,
            path,
            hash: String::new(),
            roles: Vec::new(),
        })
        .roles
        .push(role);
}

pub(super) fn resolve(install: &Path, hash: &mut blake3::Hasher) -> Result<Vec<Source>, Error> {
    let directory = install.join("widgetui/textures/ingame");
    let mut sources = BTreeMap::new();
    let bytes = read_bounded(&install.join("resources/_common/dat/empires2_x2_p1.dat"))?;
    let dat = crate::dat::de_survey::inflate(&bytes)?;
    let ids: Vec<_> = crate::catalog::GAMEPLAY_BUILDING_IDS
        .iter()
        .filter(|&&id| !(605..=610).contains(&id))
        .map(|&id| id as i16)
        .collect();
    let resources = [59, 349, 66, 102, 53];
    let wanted: Vec<_> = ids.iter().copied().chain(resources).collect();
    let units = crate::dat::de_units::find(&dat, &wanted)?;
    let buildings = named_inventory(&directory.join("buildings"), None)?;
    for id in ids {
        let icon = units[&id].icon;
        let path = buildings.get(&icon).ok_or_else(|| {
            invalid(
                "DE interface",
                0,
                format!("building {id} missing icon {icon}"),
            )
        })?;
        add(
            &mut sources,
            format!("de/interface/buildings/{icon}.dds"),
            path.clone(),
            Role::Unit(i32::from(id)),
        );
    }
    let unit_icons = inventory(&directory.join("units"))?;
    for id in resources {
        let icon = units[&id].icon;
        let path = unit_icons.get(&icon).ok_or_else(|| {
            invalid(
                "DE interface",
                0,
                format!("resource {id} missing icon {icon}"),
            )
        })?;
        add(
            &mut sources,
            format!("de/interface/resource-icons/{icon}.dds"),
            path.clone(),
            Role::Unit(i32::from(id)),
        );
    }
    for (name, index) in [
        ("cancel", 0),
        ("delete", 1),
        ("ungarrison", 2),
        ("stop", 3),
        ("repair", 28),
        ("build", 30),
        ("military", 31),
        ("move", 32),
        ("attack", 41),
        ("garrison", 4),
        ("build-cursor", 30),
        ("formation-line", 35),
        ("formation-box", 43),
        ("formation-staggered", 62),
        ("formation-flank", 63),
        ("attack-move", 84),
        ("auto-scout", 74),
    ] {
        let file = format!("actions/{index:03}_.png");
        add(
            &mut sources,
            format!("de/interface/{file}"),
            directory.join(file),
            Role::Command(name),
        );
    }
    for (name, file) in [
        ("food", "resource_food"),
        ("wood", "resource_wood"),
        ("stone", "resource_stone"),
        ("gold", "resource_gold"),
        ("population", "pop"),
    ] {
        let file = format!("icons/{file}.png");
        add(
            &mut sources,
            format!("de/interface/{file}"),
            directory.join(file),
            Role::Resource(name),
        );
    }
    for (file, roles) in [
        (
            "panels/WEST/command-panel_extended.png",
            ["western-commands", "celtic-commands"],
        ),
        (
            "panels/WEST/single-selection-panel.png",
            ["western-selection", "celtic-selection"],
        ),
        (
            "panels/WEST/map-panel.png",
            ["western-minimap", "celtic-minimap"],
        ),
        ("panels/WEST/topbar.png", ["western-top", "celtic-top"]),
    ] {
        for role in roles {
            add(
                &mut sources,
                format!("de/interface/{file}"),
                directory.join(file),
                Role::Panel(role),
            );
        }
    }
    for (file, role) in [
        ("panels/blank_icon.png", "button"),
        ("panels/frame_regular.png", "portrait"),
    ] {
        add(
            &mut sources,
            format!("de/interface/{file}"),
            directory.join(file),
            Role::Panel(role),
        );
    }
    add(
        &mut sources,
        "de/interface/menu-background.dds".into(),
        install.join("widgetui/textures/backgrounds/mainmenu_bg.dds"),
        Role::Panel("menu-background"),
    );
    let mut sources: Vec<_> = sources.into_values().collect();
    hash.update(b"DE native interface roles\0");
    for source in &mut sources {
        let bytes = read_bounded(&source.path)?;
        source.hash = blake3::hash(&bytes).to_hex().to_string();
        hash.update(source.id.as_bytes());
        hash.update(&serde_json::to_vec(&source.roles)?);
        hash.update(source.hash.as_bytes());
    }
    Ok(sources)
}

pub(super) fn frame(source: &Source, bytes: &[u8]) -> Result<Planes, Error> {
    if source
        .roles
        .iter()
        .any(|role| matches!(role, Role::Panel("menu-background")))
    {
        background(bytes)
    } else {
        decode(bytes, source.id.ends_with(".png"))
    }
}

/// Average one BC1 block at a time into a bounded private menu image. The
/// complete source image is never decoded or allocated; DDS terrain and icon
/// decoders retain their own limits and native-resolution policies.
pub(super) fn background(bytes: &[u8]) -> Result<Planes, Error> {
    const FORMAT: &str = "DE menu DDS";
    crate::slice(bytes, 0, 128, FORMAT)?;
    if &bytes[..4] != b"DDS " || crate::u32_at(bytes, 4, FORMAT)? != 124 {
        return Err(invalid(FORMAT, 0, "DDS header"));
    }
    let (width, height) = (
        crate::u32_at(bytes, 16, FORMAT)?,
        crate::u32_at(bytes, 12, FORMAT)?,
    );
    if width == 0
        || height == 0
        || width > 8192
        || height > 4096
        || !width.is_multiple_of(4)
        || !height.is_multiple_of(4)
    {
        return Err(invalid(FORMAT, 12, "background dimensions"));
    }
    if &bytes[84..88] != b"DXT1"
        || crate::u32_at(bytes, 24, FORMAT)? > 1
        || crate::u32_at(bytes, 112, FORMAT)? != 0
    {
        return Err(invalid(FORMAT, 24, "unsupported background layout"));
    }
    let (wide, high) = (width as usize / 4, height as usize / 4);
    let data = crate::slice(bytes, 128, wide * high * 8, FORMAT)?;
    let mut planes = Planes::blank(wide as u16, high as u16, 0, 0);
    for (out, raw) in planes
        .color
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(data.as_chunks::<8>().0)
    {
        let pixels = crate::sld::bc1_block(*raw);
        for (channel, output) in out.iter_mut().enumerate() {
            *output = (pixels
                .iter()
                .map(|pixel| u16::from(pixel[channel]))
                .sum::<u16>()
                / 16) as u8;
        }
    }
    Ok(planes)
}

pub(super) fn decode(bytes: &[u8], is_png: bool) -> Result<Planes, Error> {
    if !is_png {
        return super::decode(bytes);
    }
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_limits(png::Limits {
        bytes: 16 * 1024 * 1024,
    });
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info()?;
    let (width, height) = (reader.info().width, reader.info().height);
    if width == 0
        || height == 0
        || width > 2048
        || height > 2048
        || width
            .checked_mul(height)
            .is_none_or(|pixels| pixels > 4 * 1024 * 1024)
        || reader.output_buffer_size() > 16 * 1024 * 1024
    {
        return Err(invalid("DE interface", 0, "PNG dimensions"));
    }
    let mut pixels = vec![0; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut pixels)?;
    if frame.bit_depth != png::BitDepth::Eight {
        return Err(invalid("DE interface", 0, "PNG bit depth"));
    }
    let mut planes = Planes::blank(width as u16, height as u16, 0, 0);
    let channels = frame.color_type.samples();
    for (out, pixel) in planes
        .color
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(pixels[..frame.buffer_size()].chunks_exact(channels))
    {
        let rgba = match frame.color_type {
            png::ColorType::Rgba => [pixel[0], pixel[1], pixel[2], pixel[3]],
            png::ColorType::Rgb => [pixel[0], pixel[1], pixel[2], 255],
            png::ColorType::Grayscale => [pixel[0], pixel[0], pixel[0], 255],
            png::ColorType::GrayscaleAlpha => [pixel[0], pixel[0], pixel[0], pixel[1]],
            png::ColorType::Indexed => {
                return Err(invalid("DE interface", 0, "unexpanded PNG palette"));
            }
        };
        out.copy_from_slice(&rgba);
    }
    Ok(planes)
}

pub(super) fn apply(
    manifest: &Manifest,
    sources: &[Source],
    out: &mut Interface,
) -> Result<(), Error> {
    for source in sources {
        let frame = manifest
            .frames
            .iter()
            .find(|f| f.source == source.id && f.frame == 0 && f.native.is_none())
            .ok_or_else(|| invalid("DE interface", 0, "missing native UI cell"))?;
        let page = manifest
            .pages
            .get(usize::from(frame.page))
            .ok_or_else(|| invalid("DE interface", 0, "missing UI page"))?;
        if frame.width == 0
            || frame.height == 0
            || u32::from(frame.x) + u32::from(frame.width) > u32::from(page.width)
            || u32::from(frame.y) + u32::from(frame.height) > u32::from(page.height)
        {
            return Err(invalid("DE interface", 0, "UI crop bounds"));
        }
        let art = Art {
            page: page.color.clone(),
            x: frame.x,
            y: frame.y,
            width: frame.width,
            height: frame.height,
            page_width: page.width,
            page_height: page.height,
        };
        for role in &source.roles {
            match role {
                Role::Unit(id) => {
                    out.units.insert(*id, art.clone());
                }
                Role::Command(name) => {
                    out.commands.insert((*name).into(), art.clone());
                }
                Role::Resource(name) => {
                    out.resources.insert((*name).into(), art.clone());
                }
                Role::Panel(name) => {
                    out.panels.insert((*name).into(), art.clone());
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;

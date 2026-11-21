//! Decode installed particle cells into the same planes as SLD actors.
use super::{Error, Path, Pose, Selected, de_survey::graphics::Particle, de_units, invalid};
use crate::{dds::Texture, pack::atlas::Planes};
use serde::Deserialize;

const PARTICLES: &str = "resources/_common/particles";
const TEXTURE_PIXELS: usize = 4096 * 4096;

#[derive(Clone, Debug)]
pub(crate) struct Sheet {
    pub name: String,
    source: Source,
    stride: usize,
    offset: [i32; 2],
    start_scale: f32,
    end_scale: f32,
    start_alpha: f32,
    end_alpha: f32,
}

#[derive(Clone, Debug)]
enum Source {
    Atlas { path: String, cells: Vec<Cell> },
    Sequence(Vec<String>),
}

#[derive(Clone, Debug, Deserialize)]
struct Rect {
    x: u16,
    y: u16,
    w: u16,
    h: u16,
}

#[derive(Clone, Debug, Deserialize)]
struct Size {
    w: u16,
    h: u16,
}

#[derive(Clone, Debug, Deserialize)]
struct Pivot {
    x: f32,
    y: f32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Cell {
    frame: Rect,
    rotated: bool,
    sprite_source_size: Rect,
    source_size: Size,
    pivot: Pivot,
}

#[derive(Deserialize)]
struct Atlas {
    frames: Vec<Cell>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Sequence {
    format: String,
    first: usize,
    last: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Emitter {
    atlas_file: Option<String>,
    #[serde(default)]
    image_first: usize,
    image_count: Option<usize>,
    image_angles: Option<u16>,
    atlas_images_raw: Option<Sequence>,
    duration: Option<f32>,
    duration1: Option<f32>,
    duration2: Option<f32>,
    scale: Option<f32>,
    scale_start1: Option<f32>,
    scale_start2: Option<f32>,
    scale_end1: Option<f32>,
    scale_end2: Option<f32>,
    alpha: Option<f32>,
    alpha_start: Option<f32>,
    alpha_end: Option<f32>,
}

pub(crate) struct ParticleFrames {
    pub(crate) sheet: Sheet,
    pub(crate) frames: u16,
    pub(crate) angles: u16,
    pub(crate) seconds: f32,
}

fn safe_path(path: &str) -> bool {
    path.split('/').all(|part| {
        !part.is_empty()
            && part.as_bytes().first() != Some(&b'.')
            && part
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
    })
}

fn source(
    read: impl Fn(&str) -> Result<Vec<u8>, Error>,
    emitter: &Emitter,
) -> Result<(Source, usize), Error> {
    let bad = || invalid("DE particle", 0, "invalid particle source");
    if let Some(atlas) = &emitter.atlas_file {
        let path = atlas.replace('\\', "/");
        let (stem, extension) = path.rsplit_once('.').ok_or_else(bad)?;
        if !safe_path(&path)
            || !path.starts_with("textures/atlases/")
            || !matches!(extension, "dds" | "png")
        {
            return Err(bad());
        }
        let layout: Atlas = serde_json::from_slice(&read(&format!("{stem}.json"))?)?;
        let cells = emitter
            .image_count
            .and_then(|count| emitter.image_first.checked_add(count))
            .and_then(|end| layout.frames.get(emitter.image_first..end))
            .ok_or_else(bad)?
            .to_vec();
        let count = cells.len();
        Ok((Source::Atlas { path, cells }, count))
    } else if let Some(sequence) = &emitter.atlas_images_raw {
        let format = sequence.format.replace('\\', "/");
        let (prefix, suffix) = format.split_once("%02d").ok_or_else(bad)?;
        if suffix != ".png"
            || !safe_path(prefix)
            || !prefix.starts_with("textures/")
            || sequence.last > 99
        {
            return Err(bad());
        }
        let paths: Vec<_> = (sequence.first..=sequence.last)
            .map(|i| format!("{prefix}{i:02}{suffix}"))
            .collect();
        let count = paths.len();
        Ok((Source::Sequence(paths), count))
    } else {
        Err(bad())
    }
}

pub(crate) fn read(install: &Path, name: &str, offset: [i32; 2]) -> Result<ParticleFrames, Error> {
    let bad = || invalid("DE particle", 0, format!("invalid emitter {name}"));
    if !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') || name.is_empty() {
        return Err(bad());
    }
    let root = install.join(PARTICLES);
    let read = |path: &str| super::super::read_bounded(&root.join(path));
    let emitter: Emitter = serde_json::from_slice(&read(&format!("{name}.json"))?)?;
    let (source, count) = source(read, &emitter)?;
    let angles = emitter.image_angles.unwrap_or(1);
    let mean = |a, b| match (a, b) {
        (Some(a), Some(b)) => Some((a + b) / 2.0),
        _ => None,
    };
    let duration = emitter
        .duration
        .or_else(|| mean(emitter.duration1, emitter.duration2))
        .ok_or_else(bad)?;
    let scale = |a, b| match (emitter.scale, a, b) {
        (Some(scale), _, _) => Ok(scale),
        (None, None, None) => Ok(1.0),
        _ => mean(a, b).ok_or_else(bad),
    };
    let start_scale = scale(emitter.scale_start1, emitter.scale_start2)?;
    let end_scale = scale(emitter.scale_end1, emitter.scale_end2)?;
    let start_alpha = emitter.alpha_start.or(emitter.alpha).unwrap_or(1.0);
    let end_alpha = emitter.alpha_end.or(emitter.alpha).unwrap_or(1.0);
    if !matches!(angles, 1 | 16 | 32)
        || !count.is_multiple_of(usize::from(angles))
        || !(1..=128).contains(&(count / usize::from(angles)))
        || !(0.01..=60.0).contains(&duration)
        || [start_scale, end_scale]
            .into_iter()
            .any(|v| !(0.01..=1.0).contains(&v))
        || [start_alpha, end_alpha]
            .into_iter()
            .any(|v| !(0.0..=1.0).contains(&v))
    {
        return Err(bad());
    }
    let frames = count as u16 / angles;
    Ok(ParticleFrames {
        frames,
        angles,
        seconds: duration / f32::from(frames),
        sheet: Sheet {
            name: name.to_owned(),
            source,
            stride: usize::from(frames),
            offset,
            start_scale,
            end_scale,
            start_alpha,
            end_alpha,
        },
    })
}

pub(super) fn select(
    install: &Path,
    unit: &de_units::Unit,
    graphic: i16,
    pose: Pose,
    particle: &Particle,
) -> Result<Selected, Error> {
    let ParticleFrames {
        sheet,
        frames,
        angles,
        seconds,
    } = read(install, &particle.name, particle.offset)?;
    let effect = particle.graphic;
    Ok(Selected {
        unit: unit.id,
        icon: unit.icon,
        pose: pose as usize,
        graphic,
        sprite: 200_000 + effect as u32,
        file: format!("dat_{effect}"),
        frames,
        angles,
        cells: frames * angles,
        seconds,
        replay: 0.0,
        terminal: false,
        parts: Vec::new(),
        particle: Some(sheet),
    })
}

#[cfg(test)]
mod tests;

impl Sheet {
    pub(crate) fn len(&self) -> usize {
        match &self.source {
            Source::Atlas { cells, .. } => cells.len(),
            Source::Sequence(paths) => paths.len(),
        }
    }

    pub(crate) fn hash(&self, install: &Path) -> Result<String, Error> {
        let mut hash = blake3::Hasher::new();
        hash.update(format!("{self:?}").as_bytes());
        for path in self.paths(install) {
            hash.update(&super::super::read_bounded(&path)?);
        }
        Ok(hash.finalize().to_hex().to_string())
    }

    pub(crate) fn paths(&self, install: &Path) -> impl Iterator<Item = std::path::PathBuf> {
        let paths: &[String] = match &self.source {
            Source::Atlas { path, .. } => std::slice::from_ref(path),
            Source::Sequence(paths) => paths,
        };
        let root = install.join(PARTICLES);
        paths.iter().map(move |path| root.join(path))
    }

    pub(crate) fn load(&self, install: &Path) -> Result<Vec<Texture>, Error> {
        let mut pixels = 0;
        self.paths(install)
            .map(|path| {
                let bytes = super::super::read_bounded(&path)?;
                let texture = if path.extension().is_some_and(|e| e == "dds") {
                    crate::dds::parse(&bytes)?
                } else {
                    super::super::icons::particle_texture(&bytes)?
                };
                pixels += texture.pixels.len();
                if pixels > TEXTURE_PIXELS {
                    return Err(invalid("DE particle", 0, "texture pixel bound"));
                }
                Ok(texture)
            })
            .collect()
    }

    pub(crate) fn frame(&self, textures: &[Texture], index: usize) -> Result<Planes, Error> {
        let bad = || invalid("DE particle", index, "invalid atlas cell");
        let (texture, cell) = match &self.source {
            Source::Atlas { cells, .. } => (
                textures.first().ok_or_else(bad)?,
                cells.get(index).ok_or_else(bad)?.clone(),
            ),
            Source::Sequence(_) => {
                let texture = textures.get(index).ok_or_else(bad)?;
                let frame = Rect {
                    x: 0,
                    y: 0,
                    w: texture.width,
                    h: texture.height,
                };
                (
                    texture,
                    Cell {
                        frame: frame.clone(),
                        rotated: false,
                        sprite_source_size: frame,
                        source_size: Size {
                            w: texture.width,
                            h: texture.height,
                        },
                        pivot: Pivot { x: 0.5, y: 0.5 },
                    },
                )
            }
        };
        let Rect { x, y, w, h } = cell.frame;
        let (packed_w, packed_h) = if cell.rotated { (h, w) } else { (w, h) };
        if w == 0
            || h == 0
            || u32::from(x) + u32::from(packed_w) > u32::from(texture.width)
            || u32::from(y) + u32::from(packed_h) > u32::from(texture.height)
            || (w, h) != (cell.sprite_source_size.w, cell.sprite_source_size.h)
            || [cell.pivot.x, cell.pivot.y]
                .into_iter()
                .any(|p| !(0.0..=1.0).contains(&p))
        {
            return Err(bad());
        }
        let phase = (index % self.stride) as f32 / self.stride as f32;
        let scale = self.start_scale + (self.end_scale - self.start_scale) * phase;
        let alpha = self.start_alpha + (self.end_alpha - self.start_alpha) * phase;
        let width = (f32::from(w) * scale).round().max(1.0) as u16;
        let height = (f32::from(h) * scale).round().max(1.0) as u16;
        let anchor_x =
            f32::from(cell.source_size.w) * cell.pivot.x - f32::from(cell.sprite_source_size.x);
        let anchor_y =
            f32::from(cell.source_size.h) * cell.pivot.y - f32::from(cell.sprite_source_size.y);
        let mut out = Planes::blank(
            width,
            height,
            (anchor_x * scale).round() as i32 - self.offset[0],
            (anchor_y * scale).round() as i32 - self.offset[1],
        );
        for dy in 0..height {
            for dx in 0..width {
                let sx = (u32::from(dx) * u32::from(w) / u32::from(width)) as u16;
                let sy = (u32::from(dy) * u32::from(h) / u32::from(height)) as u16;
                let (sx, sy) = if cell.rotated {
                    (h - 1 - sy, sx)
                } else {
                    (sx, sy)
                };
                let mut pixel = texture.pixels
                    [usize::from(y + sy) * usize::from(texture.width) + usize::from(x + sx)];
                pixel[3] = (f32::from(pixel[3]) * alpha).round() as u8;
                let to = (usize::from(dy) * usize::from(width) + usize::from(dx)) * 4;
                out.color[to..to + 4].copy_from_slice(&pixel);
            }
        }
        Ok(out)
    }
}

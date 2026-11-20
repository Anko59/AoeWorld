//! The same compositor handles every drawable DAT graphic and delta layer.
use super::{GRAPHICS, compose, read_bounded, unit_catalog::Selected};
use crate::{Error, invalid, pack::atlas::Planes, sld};
use std::path::Path;
pub(super) fn hash(install: &Path, source: &Selected) -> Result<String, Error> {
    let mut hash = blake3::Hasher::new();
    hash.update(format!("{source:?}").as_bytes());
    for part in &source.parts {
        let bytes = read_bounded(
            &install
                .join(GRAPHICS)
                .join(format!("{}.sld", part.animation.file)),
        )?;
        hash.update(&bytes);
    }
    if let Some(particle) = &source.particle {
        for path in particle.paths(install) {
            hash.update(&read_bounded(&path)?);
        }
    }
    Ok(hash.finalize().to_hex().to_string())
}
pub(super) struct Decoder {
    source: Option<Selected>,
    parts: Vec<Cells>,
    textures: Vec<crate::dds::Texture>,
}

enum Cells {
    Sequential(sld::Stream),
    Repeated(Vec<sld::Frame>),
}

impl Cells {
    fn len(&self) -> usize {
        match self {
            Self::Sequential(stream) => stream.len(),
            Self::Repeated(frames) => frames.len(),
        }
    }

    fn frame(&mut self, index: usize) -> Result<&sld::Frame, Error> {
        match self {
            Self::Sequential(stream) => stream.frame(index),
            Self::Repeated(frames) => frames
                .get(index)
                .ok_or_else(|| invalid("DE unit", index, "missing delta cell")),
        }
    }
}

impl Decoder {
    pub fn streamed(bytes: Vec<u8>) -> Result<Self, Error> {
        Ok(Self {
            source: None,
            parts: vec![Cells::Sequential(sld::Stream::new(bytes)?)],
            textures: Vec::new(),
        })
    }
    pub fn new(install: &Path, source: &Selected) -> Result<Self, Error> {
        if let Some(particle) = &source.particle {
            return Ok(Self {
                source: Some(source.clone()),
                parts: Vec::new(),
                textures: particle.load(install)?,
            });
        }
        let mut parts = Vec::new();
        let mut retained_bytes = 0usize;
        let last_phase = f64::from(source.frames.saturating_sub(1)) * f64::from(source.seconds);
        for part in &source.parts {
            let a = &part.animation;
            let path = install.join(GRAPHICS).join(format!("{}.sld", a.file));
            let bytes = read_bounded(&path)?;
            // Direction-independent layers repeat across rows. Shorter looping
            // layers also need random access. Stream the rest one cell at a time.
            let repeated = source.parts.len() > 1
                && a.frames > 1
                && (a.angles == 1
                    || (a.seconds > 0.0
                        && (a.frames != source.frames || a.seconds != source.seconds)
                        && last_phase
                            >= f64::from(a.frames) * f64::from(a.seconds)
                                + f64::from(a.replay_delay)));
            let cells = if repeated {
                let frames = sld::parse(&bytes).inspect_err(|_| {
                    eprintln!("SLD decode failed: {}", path.display());
                })?;
                retained_bytes += frames
                    .iter()
                    .flat_map(|f| [&f.main, &f.shadow, &f.player].into_iter().flatten())
                    .map(|l| l.pixels.len() * 4)
                    .sum::<usize>();
                Cells::Repeated(frames)
            } else {
                retained_bytes += bytes.len();
                Cells::Sequential(sld::Stream::new(bytes)?)
            };
            if retained_bytes > 256 * 1024 * 1024 {
                return Err(invalid("DE unit", 0, "delta source memory bound"));
            }
            parts.push(cells);
        }
        Ok(Self {
            source: Some(source.clone()),
            textures: Vec::new(),
            parts,
        })
    }
    pub fn len(&self) -> usize {
        self.source
            .as_ref()
            .map_or_else(|| self.parts[0].len(), |s| usize::from(s.cells))
    }
    pub fn frame(&mut self, index: usize) -> Result<Planes, Error> {
        if let Some(particle) = self.source.as_ref().and_then(|s| s.particle.as_ref()) {
            return particle.frame(&self.textures, index);
        }
        if let [part] = self.parts.as_mut_slice() {
            return part.frame(index).map(compose::planes);
        }
        let source = self
            .source
            .as_ref()
            .ok_or_else(|| invalid("DE unit", 0, "missing composite descriptor"))?;
        let row = index / usize::from(source.frames);
        let phase = index % usize::from(source.frames);
        let mut planes = Vec::new();
        let last = index + 1 == usize::from(source.cells);
        for (part, frames) in source.parts.iter().zip(&mut self.parts) {
            let a = &part.animation;
            let layer_phase = if a.frames == source.frames && a.seconds == source.seconds {
                phase
            } else if a.frames == 1 || a.seconds == 0.0 {
                0
            } else {
                // Sails and the hull can have their own DAT clocks.
                let elapsed = phase as f64 * f64::from(source.seconds);
                let seconds = f64::from(a.seconds);
                let period = f64::from(a.frames) * seconds + f64::from(a.replay_delay);
                ((elapsed % period / seconds) as usize).min(usize::from(a.frames - 1))
            };
            let index = if a.angles == 1 { 0 } else { row } * usize::from(a.frames) + layer_phase;
            let mut p = compose::planes(frames.frame(index)?);
            if last {
                // Validate trailing native cells even when this compositor
                // does not sample them.
                frames.frame(frames.len() - 1)?;
            }
            p.hotspot_x -= part.x;
            p.hotspot_y -= part.y;
            planes.push(p);
        }
        merge(&planes)
    }
}

fn merge(parts: &[Planes]) -> Result<Planes, Error> {
    let left = parts.iter().map(|p| -p.hotspot_x).min().unwrap_or(0);
    let top = parts.iter().map(|p| -p.hotspot_y).min().unwrap_or(0);
    let right = parts
        .iter()
        .map(|p| i32::from(p.width) - p.hotspot_x)
        .max()
        .unwrap_or(1);
    let bottom = parts
        .iter()
        .map(|p| i32::from(p.height) - p.hotspot_y)
        .max()
        .unwrap_or(1);
    if !(1..=2044).contains(&(right - left)) || !(1..=2044).contains(&(bottom - top)) {
        return Err(invalid("DE unit", 0, "composite extent bound"));
    }
    let mut out = Planes::blank((right - left) as u16, (bottom - top) as u16, -left, -top);
    for p in parts {
        for y in 0..usize::from(p.height) {
            for x in 0..usize::from(p.width) {
                let from = (y * usize::from(p.width) + x) * 4;
                let dx = x as i32 - p.hotspot_x - left;
                let dy = y as i32 - p.hotspot_y - top;
                let to = (dy as usize * usize::from(out.width) + dx as usize) * 4;
                // SLD colour coverage is binary. Its BC4 player weight remains exact.
                if p.color[from + 3] != 0 {
                    out.color[to..to + 4].copy_from_slice(&p.color[from..from + 4]);
                    out.player[to..to + 4].copy_from_slice(&p.player[from..from + 4]);
                }
                out.shadow[to + 3] = out.shadow[to + 3].max(p.shadow[from + 3]);
            }
        }
    }
    Ok(out)
}
#[cfg(test)]
mod tests;

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
    Ok(hash.finalize().to_hex().to_string())
}
pub(super) struct Decoder {
    source: Option<Selected>,
    parts: Vec<Vec<sld::Frame>>,
    stream: Option<sld::Stream>,
}
impl Decoder {
    pub fn streamed(bytes: Vec<u8>) -> Result<Self, Error> {
        Ok(Self {
            source: None,
            parts: Vec::new(),
            stream: Some(sld::Stream::new(bytes)?),
        })
    }
    pub fn new(install: &Path, source: &Selected) -> Result<Self, Error> {
        let mut parts = Vec::new();
        let mut bytes = 0usize;
        for part in &source.parts {
            let frames = sld::parse(&read_bounded(
                &install
                    .join(GRAPHICS)
                    .join(format!("{}.sld", part.animation.file)),
            )?)?;
            bytes += frames
                .iter()
                .map(|f| {
                    [&f.main, &f.shadow, &f.player]
                        .into_iter()
                        .flatten()
                        .map(|l| l.pixels.len() * 4)
                        .sum::<usize>()
                })
                .sum::<usize>();
            if bytes > 256 * 1024 * 1024 {
                return Err(invalid("DE unit", 0, "delta source memory bound"));
            }
            parts.push(frames);
        }
        Ok(Self {
            source: Some(source.clone()),
            stream: None,
            parts,
        })
    }
    pub fn len(&self) -> usize {
        if let Some(stream) = &self.stream {
            return stream.len();
        }
        self.source
            .as_ref()
            .map_or(self.parts[0].len(), |s| usize::from(s.cells))
    }
    pub fn frame(&mut self, index: usize) -> Result<Planes, Error> {
        if let Some(stream) = &mut self.stream {
            return stream.frame(index).map(compose::planes);
        }
        if self.parts.len() == 1 {
            return self.parts[0]
                .get(index)
                .map(compose::planes)
                .ok_or_else(|| invalid("DE unit", 0, "missing native cell"));
        }
        let source = self
            .source
            .as_ref()
            .ok_or_else(|| invalid("DE unit", 0, "missing composite descriptor"))?;
        let row = index / usize::from(source.frames);
        let phase = index % usize::from(source.frames);
        let mut planes = Vec::new();
        for (part, frames) in source.parts.iter().zip(&self.parts) {
            let a = &part.animation;
            let index = if a.angles == 1 { 0 } else { row } * usize::from(a.frames)
                + if a.frames == 1 { 0 } else { phase };
            let mut p = compose::planes(
                frames
                    .get(index)
                    .ok_or_else(|| invalid("DE unit", 0, "missing delta cell"))?,
            );
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

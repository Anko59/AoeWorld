//! Repaint changed regions in the original layer order, retaining exact depth/alpha.
use super::{WorldLayer, canvas_layer_bounds};
use std::cmp::Ordering;

const MAX_LAYERS: usize = 65_536;

#[derive(Default)]
pub(super) struct Damage {
    layers: Vec<WorldLayer>,
    fog: Vec<[f32; 3]>,
    viewport: [f64; 2],
    size: [u32; 2],
    backdrop: Option<bool>,
    atlas: (usize, usize),
}

impl Damage {
    pub(super) fn resident_bytes(&self) -> usize {
        self.layers.capacity() * std::mem::size_of::<WorldLayer>()
            + self.fog.capacity() * std::mem::size_of::<[f32; 3]>()
    }

    pub(super) fn invalidate(&mut self) {
        self.backdrop = None;
        self.layers.clear();
        self.fog.clear();
    }

    pub(super) fn regions(
        &self,
        layers: &[WorldLayer],
        fog: &[[f32; 3]],
        viewport: [f64; 2],
        size: [u32; 2],
        backdrop: bool,
        atlas: &[u8],
    ) -> Vec<[u32; 4]> {
        if self.backdrop != Some(backdrop)
            || self.viewport != viewport
            || self.size != size
            || self.atlas != (atlas.as_ptr() as usize, atlas.len())
            || layers.len() > MAX_LAYERS
        {
            return vec![[0, 0, size[0], size[1]]];
        }
        let mut changed = Vec::new();
        let ordered = self
            .layers
            .windows(2)
            .chain(layers.windows(2))
            .all(|pair| compare(&pair[0], &pair[1]) != Ordering::Greater);
        let (mut before, mut after) = (0, 0);
        while before < self.layers.len() || after < layers.len() {
            let old = self.layers.get(before);
            let new = layers.get(after);
            let order = match (ordered, old, new) {
                (true, Some(a), Some(b)) => compare(a, b),
                (true, Some(_), None) => Ordering::Less,
                (true, None, Some(_)) => Ordering::Greater,
                _ => Ordering::Equal,
            };
            let same =
                order == Ordering::Equal && old == new && self.fog.get(before) == fog.get(after);
            for layer in old
                .filter(|_| !same && order != Ordering::Greater)
                .into_iter()
                .chain(new.filter(|_| !same && order != Ordering::Less))
            {
                if let Some(bounds) = canvas_layer_bounds(layer, viewport, size[0], size[1]) {
                    insert(&mut changed, bounds);
                    if changed.len() > 32 {
                        return vec![[0, 0, size[0], size[1]]];
                    }
                }
            }
            if order != Ordering::Greater {
                before += 1;
            }
            if order != Ordering::Less {
                after += 1;
            }
        }
        changed
    }

    pub(super) fn remember(
        &mut self,
        layers: &[WorldLayer],
        fog: &[[f32; 3]],
        viewport: [f64; 2],
        size: [u32; 2],
        backdrop: bool,
        atlas: &[u8],
    ) {
        if layers.len() > MAX_LAYERS {
            self.invalidate();
            return;
        }
        copy_bounded(&mut self.layers, layers);
        copy_bounded(&mut self.fog, fog);
        self.viewport = viewport;
        self.size = size;
        self.backdrop = Some(backdrop);
        self.atlas = (atlas.as_ptr() as usize, atlas.len());
    }
}

fn compare(a: &WorldLayer, b: &WorldLayer) -> Ordering {
    let a = crate::game_renderer::world_layers::layer_order(a);
    let b = crate::game_renderer::world_layers::layer_order(b);
    a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2))
}

fn insert(regions: &mut Vec<[u32; 4]>, mut bounds: [u32; 4]) {
    let mut index = 0;
    while index < regions.len() {
        let other = regions[index];
        if bounds[0] < other[2]
            && other[0] < bounds[2]
            && bounds[1] < other[3]
            && other[1] < bounds[3]
        {
            bounds = union(bounds, regions.swap_remove(index));
            index = 0;
        } else {
            index += 1;
        }
    }
    regions.push(bounds);
}

fn copy_bounded<T: Copy>(target: &mut Vec<T>, source: &[T]) {
    if source.len() > target.capacity() || source.len().saturating_mul(2) < target.capacity() {
        *target = Vec::with_capacity(source.len());
    }
    target.clear();
    target.extend_from_slice(source);
}

fn union(a: [u32; 4], b: [u32; 4]) -> [u32; 4] {
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        a[2].max(b[2]),
        a[3].max(b[3]),
    ]
}

#[path = "damage/tests.rs"]
#[cfg(test)]
mod tests;

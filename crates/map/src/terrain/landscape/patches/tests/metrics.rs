use super::*;
use std::collections::VecDeque;
// Only fixtures allocate arrays. The production module has no arrays/storage
// proportional to the world. Singletons use real neighboring world queries,
// not clipped grid neighbors that would invent isolated trees on fixture edges.
pub(super) struct Fixture {
    pub size: usize,
    pub samples: Vec<DensitySample>,
    pub metrics: Metrics,
}
pub(super) fn fixture(
    p: Patches,
    size: usize,
    mask: impl Fn(i32, i32) -> Input,
    wildness: impl Fn(i64, i64) -> u16,
) -> Fixture {
    let mut m = Metrics::default();
    let mut samples = Vec::with_capacity(size * size);
    for y in 0..size {
        for x in 0..size {
            let (wx, wy) = (x as i32 - 128, y as i32 - 128);
            let s = p.sample(wx, wy, &mask, &wildness);
            m.area += usize::from(s.zone != Zone::Exterior);
            m.core += usize::from(s.zone == Zone::Core);
            m.core_trees += usize::from(s.zone == Zone::Core && s.tree);
            m.exterior_trees += usize::from(s.zone == Zone::Exterior && s.tree);
            m.trees += usize::from(s.tree);
            m.glades += usize::from(s.opening == Opening::Glade);
            m.trails += usize::from(s.opening == Opening::Trail);
            if s.tree {
                let connected = (-1..=1).any(|dy| {
                    (-1..=1).any(|dx| {
                        (dx != 0 || dy != 0) && p.sample(wx + dx, wy + dy, &mask, &wildness).tree
                    })
                });
                m.singletons += usize::from(!connected);
            }
            samples.push(s);
        }
    }
    let canopy: Vec<bool> = samples.iter().map(|s| s.zone != Zone::Exterior).collect();
    let (stands, _) = components(&canopy, size, true, true);
    m.largest_stand = stands.iter().copied().max().unwrap_or(0);
    let forest: Vec<bool> = samples
        .iter()
        .map(|s| s.zone != Zone::Exterior || s.opening != Opening::None)
        .collect();
    let (forests, _) = components(&forest, size, true, true);
    m.largest_forest = forests.iter().copied().max().unwrap_or(0);
    let (open, _) = components(&canopy, size, false, false);
    m.open = size * size - m.area;
    m.largest_open = open.iter().copied().max().unwrap_or(0);
    Fixture {
        size,
        samples,
        metrics: m,
    }
}
/// Component sizes and each cell's component index (`usize::MAX` elsewhere).
pub(super) fn components(
    cells: &[bool],
    size: usize,
    target: bool,
    diagonal: bool,
) -> (Vec<usize>, Vec<usize>) {
    let mut label = vec![usize::MAX; cells.len()];
    let mut sizes = Vec::new();
    let mut queue = VecDeque::new();
    for root in 0..cells.len() {
        if label[root] != usize::MAX || cells[root] != target {
            continue;
        }
        label[root] = sizes.len();
        queue.push_back(root);
        let mut count = 0;
        while let Some(i) = queue.pop_front() {
            count += 1;
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    if (dx == 0 && dy == 0) || (!diagonal && dx != 0 && dy != 0) {
                        continue;
                    }
                    let (x, y) = ((i % size) as i32 + dx, (i / size) as i32 + dy);
                    if x < 0 || y < 0 || x >= size as i32 || y >= size as i32 {
                        continue;
                    }
                    let n = y as usize * size + x as usize;
                    if label[n] == usize::MAX && cells[n] == target {
                        label[n] = sizes.len();
                        queue.push_back(n);
                    }
                }
            }
        }
        sizes.push(count);
    }
    (sizes, label)
}

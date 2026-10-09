use super::*;
use std::collections::VecDeque;
// Only fixtures allocate arrays. The production module has no arrays/storage
// proportional to the world. Singletons use real neighboring world queries,
// not clipped grid neighbors that would invent isolated trees on fixture edges.
pub(super) fn fixture(p: Patches, size: usize, mask: impl Fn(i32, i32) -> Input) -> Metrics {
    let mut m = Metrics::default();
    let mut trees = vec![false; size * size];
    for y in 0..size {
        for x in 0..size {
            let (wx, wy) = (x as i32 - 128, y as i32 - 128);
            let s = p.sample(wx, wy, &mask);
            trees[y * size + x] = s.tree;
            m.area += usize::from(s.zone != Zone::Exterior);
            m.core += usize::from(s.zone == Zone::Core);
            m.core_trees += usize::from(s.zone == Zone::Core && s.tree);
            m.exterior += usize::from(s.zone == Zone::Exterior);
            m.exterior_trees += usize::from(s.zone == Zone::Exterior && s.tree);
            m.trees += usize::from(s.tree);
            if s.tree {
                let connected = (-1..=1).any(|dy| {
                    (-1..=1)
                        .any(|dx| (dx != 0 || dy != 0) && p.sample(wx + dx, wy + dy, &mask).tree)
                });
                m.singletons += usize::from(!connected);
            }
        }
    }
    m.largest_tree = component(&trees, size, true, true);
    m.largest_open = component(&trees, size, false, false);
    m
}
fn component(trees: &[bool], size: usize, target: bool, diagonal: bool) -> usize {
    let mut seen = vec![false; trees.len()];
    let mut queue = VecDeque::new();
    let mut largest = 0;
    for root in 0..trees.len() {
        if seen[root] || trees[root] != target {
            continue;
        }
        seen[root] = true;
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
                    if !seen[n] && trees[n] == target {
                        seen[n] = true;
                        queue.push_back(n);
                    }
                }
            }
        }
        largest = largest.max(count);
    }
    largest
}

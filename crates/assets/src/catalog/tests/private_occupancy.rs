//! Exact per-texel overlap proof with bounded memory instead of pairwise scans.
use super::*;
use crate::catalog::packing::Placement;
#[derive(Default)]
pub(super) struct Occupied(std::collections::BTreeMap<u16, Vec<u64>>);
impl Occupied {
    pub fn insert(&mut self, p: Placement) -> Result<(), String> {
        if p.page >= 64
            || usize::from(p.x) + usize::from(p.width) > usize::from(PAGE_SIDE)
            || usize::from(p.y) + usize::from(p.height) > usize::from(PAGE_SIDE)
        {
            return Err("invalid occupancy bounds".into());
        }
        let words = usize::from(PAGE_SIDE) / 64;
        let page = self
            .0
            .entry(p.page)
            .or_insert_with(|| vec![0; usize::from(PAGE_SIDE) * words]);
        for y in usize::from(p.y)..usize::from(p.y) + usize::from(p.height) {
            let end = usize::from(p.x) + usize::from(p.width);
            let mut x = usize::from(p.x);
            while x < end {
                let bits = (end - x).min(64 - x % 64);
                let mask = (u64::MAX >> (64 - bits)) << (x % 64);
                let word = &mut page[y * words + x / 64];
                if *word & mask != 0 {
                    return Err("overlapping runtime rectangles".into());
                }
                *word |= mask;
                x += bits;
            }
        }
        Ok(())
    }
}
#[test]
fn occupancy_proof_checks_cross_word_edges_and_distinct_pages_exactly() {
    let p = Placement {
        page: 2,
        x: 63,
        y: 2,
        width: 66,
        height: 2,
    };
    let mut occupied = Occupied::default();
    occupied.insert(p).unwrap();
    assert!(
        occupied
            .insert(Placement {
                x: 128,
                width: 1,
                ..p
            })
            .is_err()
    );
    occupied.insert(Placement { x: 129, ..p }).unwrap();
    occupied.insert(Placement { page: 4, ..p }).unwrap();
    assert!(occupied.insert(Placement { page: 64, ..p }).is_err());
}

//! Stable ordering through small indices, not a second large-element scene.
use std::cmp::Ordering;

pub(crate) fn sort_by<T: Copy>(items: &mut [T], mut compare: impl FnMut(&T, &T) -> Ordering) {
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_unstable_by(|&left, &right| {
        compare(&items[left], &items[right]).then(left.cmp(&right))
    });
    // order[new_position] = original_position. Copy each permutation cycle
    // with one saved element; marking an index as itself needs no visited Vec.
    for start in 0..order.len() {
        if order[start] == start {
            continue;
        }
        let saved = items[start];
        let mut position = start;
        loop {
            let next = order[position];
            order[position] = position;
            if next == start {
                items[position] = saved;
                break;
            }
            items[position] = items[next];
            position = next;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug)]
    struct Entry {
        depth: f64,
        tier: u8,
        id: u64,
        payload: u32,
    }

    fn compare(left: &Entry, right: &Entry) -> Ordering {
        left.depth
            .total_cmp(&right.depth)
            .then(left.tier.cmp(&right.tier))
            .then(left.id.cmp(&right.id))
    }

    fn reference(input: &[Entry]) {
        let mut expected = input.to_vec();
        expected.sort_by(compare);
        let mut actual = input.to_vec();
        sort_by(&mut actual, compare);
        assert_eq!(
            actual
                .iter()
                .map(|e| (e.depth.to_bits(), e.tier, e.id, e.payload))
                .collect::<Vec<_>>(),
            expected
                .iter()
                .map(|e| (e.depth.to_bits(), e.tier, e.id, e.payload))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn identity_two_cycles_three_cycles_and_repeated_keys_match_stable_reference() {
        for keys in [
            vec![],
            vec![0],
            vec![0, 1, 2],
            vec![1, 0],
            vec![2, 0, 1],
            vec![1, 2, 0],
            vec![3, 1, 2, 0],
            vec![2, 2, 0, 1, 2, 0, 1, 2],
        ] {
            let entries = keys
                .iter()
                .enumerate()
                .map(|(payload, &key)| Entry {
                    depth: f64::from(key),
                    tier: 2,
                    id: 7,
                    payload: payload as u32,
                })
                .collect::<Vec<_>>();
            reference(&entries);
        }
    }

    #[test]
    fn shadow_body_interleaves_equal_ids_tiers_signed_zero_and_nan_match_reference() {
        let depths = [
            0.0,
            -0.0,
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::from_bits(0x7ff8_0000_0000_0001),
            f64::from_bits(0xfff8_0000_0000_0002),
            8.0,
            8.0,
        ];
        let mut entries = Vec::new();
        for group in 0..64 {
            for tier in [2, 0, 2, 1, 2, 2] {
                let depth = depths[group % depths.len()];
                let id = if tier == 2 { (group % 3) as u64 } else { 0 };
                entries.push(Entry {
                    depth,
                    tier,
                    id,
                    payload: entries.len() as u32,
                });
            }
        }
        reference(&entries);
        entries.reverse();
        reference(&entries);
    }

    #[test]
    fn deterministic_mixed_orders_match_frozen_stable_reference() {
        let mut seed = 0x17a0_5d21_u32;
        for count in [0, 1, 2, 3, 31, 128, 1024] {
            let entries = (0..count)
                .map(|payload| {
                    seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                    Entry {
                        depth: f64::from(seed % 17),
                        tier: (seed % 3) as u8,
                        id: u64::from(seed % 5),
                        payload,
                    }
                })
                .collect::<Vec<_>>();
            reference(&entries);
        }
    }
}

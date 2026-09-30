//! Compact coordinate lookup for bounded displayed geometry and contact buckets.
//! Stores each key once; value vectors preserve their original insertion order.
//! Callers bound unique keys by the displayed mesh, never the resident world.
//! The noninlined entry/get boundary shares one lookup implementation across
//! material ownership and contact buckets instead of value-specific map code.
//! Geometrically growing keys, slots and parallel values may have a higher
//! transient heap peak than a BTreeMap; this is not a blanket RAM reduction.
#[derive(Default)]
pub struct WorldKeyIndex {
    keys: Vec<[i32; 2]>,
    slots: Vec<u32>,
}
impl WorldKeyIndex {
    fn hash(key: [i32; 2]) -> usize {
        let mut value =
            (key[0] as u32).wrapping_mul(0x9e3779b9) ^ (key[1] as u32).wrapping_mul(0x85ebca6b);
        value ^= value >> 16;
        value = value.wrapping_mul(0xc2b2ae35);
        (value ^ (value >> 16)) as usize
    }
    fn slot(&self, key: [i32; 2]) -> usize {
        let mut slot = Self::hash(key) & (self.slots.len() - 1);
        while self.slots[slot] != 0 && self.keys[self.slots[slot] as usize - 1] != key {
            slot = (slot + 1) & (self.slots.len() - 1);
        }
        slot
    }
    /// Resolve a world key without changing insertion order or allocating.
    #[inline(never)]
    pub fn get(&self, key: [i32; 2]) -> Option<usize> {
        if self.slots.is_empty() {
            return None;
        }
        let index = self.slots[self.slot(key)];
        (index != 0).then_some(index.wrapping_sub(1) as usize)
    }
    /// Return the original index for duplicates, or append one unique key.
    #[inline(never)]
    pub fn entry(&mut self, key: [i32; 2]) -> usize {
        if let Some(index) = self.get(key) {
            return index;
        }
        if self.slots.is_empty() || (self.keys.len() + 1) * 4 > self.slots.len() * 3 {
            self.slots = vec![0; (self.slots.len() * 2).max(16)];
            for index in 0..self.keys.len() {
                let slot = self.slot(self.keys[index]);
                self.slots[slot] = index as u32 + 1;
            }
        }
        let slot = self.slot(key);
        let index = self.keys.len();
        assert!(
            index < u32::MAX as usize,
            "coordinate index exceeds u32 slots"
        );
        self.keys.push(key);
        self.slots[slot] = self.keys.len() as u32;
        index
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use wasm_bindgen_test::wasm_bindgen_test;

    #[wasm_bindgen_test]
    fn coordinate_lookup_matches_ordered_reference_and_bounds_capacity() {
        let mut actual = WorldKeyIndex::default();
        let mut expected = BTreeMap::new();
        assert_eq!(actual.get([0, 0]), None);
        for index in 0..32768_i32 {
            let key = if index % 7 == 0 {
                [index.wrapping_mul(i32::MAX), index.wrapping_mul(-123457)]
            } else {
                [index % 97 - 48, index / 97 % 193 - 96]
            };
            let next = expected.len();
            let slot = *expected.entry(key).or_insert(next);
            assert_eq!(actual.entry(key), slot);
            assert_eq!(actual.get(key), Some(slot));
            assert!(actual.slots.len() <= ((actual.keys.len() + 1) * 8 / 3).max(16));
            assert!(actual.keys.capacity() <= actual.keys.len().max(4) * 2);
        }
        for (key, slot) in expected {
            assert_eq!(actual.get(key), Some(slot));
        }
    }

    #[wasm_bindgen_test]
    fn maximum_mesh_vertex_and_contact_key_budgets_have_bounded_capacity() {
        let triangles = crate::surface_mesh::MAX_SURFACE_TRIANGLES;
        for limit in [triangles * 3, triangles * 4] {
            let mut actual = WorldKeyIndex::default();
            for index in 0..limit {
                let key = [(index % 512) as i32, (index / 512) as i32];
                assert_eq!(actual.entry(key), index);
            }
            assert_eq!(actual.keys.len(), limit);
            assert!(actual.keys.capacity() <= limit * 2);
            assert!(actual.slots.len() <= (limit + 1) * 8 / 3);
            for index in 0..limit {
                let key = [(index % 512) as i32, (index / 512) as i32];
                assert_eq!(actual.get(key), Some(index));
            }
            let capacity = (actual.keys.capacity(), actual.slots.len());
            assert_eq!(actual.entry([0, 0]), 0);
            assert_eq!(capacity, (actual.keys.capacity(), actual.slots.len()));
        }
    }

    #[wasm_bindgen_test]
    fn duplicate_and_colliding_keys_keep_original_indexes_without_extra_storage() {
        let mut actual = WorldKeyIndex::default();
        let colliding = (0..100000_i32)
            .map(|x| [x, x.wrapping_mul(7)])
            .filter(|&key| WorldKeyIndex::hash(key) & 255 == 0)
            .take(80)
            .collect::<Vec<_>>();
        assert_eq!(colliding.len(), 80);
        for (index, &key) in colliding.iter().enumerate() {
            assert_eq!(actual.entry(key), index);
        }
        let capacity = (actual.keys.capacity(), actual.slots.len());
        for (index, &key) in colliding.iter().enumerate().rev() {
            assert_eq!(actual.entry(key), index);
            assert_eq!(actual.get(key), Some(index));
        }
        assert_eq!(capacity, (actual.keys.capacity(), actual.slots.len()));
        assert_eq!(actual.keys.len(), 80);
    }
}

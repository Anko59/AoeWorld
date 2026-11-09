//! A set of entity ids as a two-level bitset. Ids are allocated sequentially
//! (and stay monotonic), so one bit per id replaces a sorted `Vec` whose
//! inserts and removals moved the tail, and a per-tick sort. Iteration is in
//! ascending id order, the order the sorted list had.
use aoe_core::EntityId;

#[derive(Clone, Debug, Default)]
pub(crate) struct IdSet {
    /// Bit `id % 64` of word `id / 64`.
    words: Vec<u64>,
    /// Bit `w % 64` of `summary[w / 64]` is set when `words[w]` is non-zero.
    summary: Vec<u64>,
    len: usize,
}

impl IdSet {
    pub(crate) fn len(&self) -> usize {
        self.len
    }

    pub(crate) fn contains(&self, id: EntityId) -> bool {
        let at = id.0 as usize;
        self.words
            .get(at / 64)
            .is_some_and(|word| word & (1 << (at % 64)) != 0)
    }

    /// Adds `id`; false when it was already present.
    pub(crate) fn insert(&mut self, id: EntityId) -> bool {
        let at = id.0 as usize;
        let word = at / 64;
        if word >= self.words.len() {
            self.words.resize(word + 1, 0);
            self.summary.resize(word / 64 + 1, 0);
        }
        let bit = 1 << (at % 64);
        if self.words[word] & bit != 0 {
            return false;
        }
        self.words[word] |= bit;
        self.summary[word / 64] |= 1 << (word % 64);
        self.len += 1;
        true
    }

    /// Removes `id`; false when it was absent.
    pub(crate) fn remove(&mut self, id: EntityId) -> bool {
        let at = id.0 as usize;
        let word = at / 64;
        let bit = 1 << (at % 64);
        let Some(value) = self.words.get_mut(word).filter(|value| **value & bit != 0) else {
            return false;
        };
        *value &= !bit;
        if *value == 0 {
            self.summary[word / 64] &= !(1 << (word % 64));
        }
        self.len -= 1;
        true
    }

    /// Empties the set, touching only the words that hold ids.
    pub(crate) fn clear(&mut self) {
        for (block, summary) in self.summary.iter_mut().enumerate() {
            let mut bits = std::mem::take(summary);
            while bits != 0 {
                self.words[block * 64 + bits.trailing_zeros() as usize] = 0;
                bits &= bits - 1;
            }
        }
        self.len = 0;
    }

    /// The ids in ascending order.
    pub(crate) fn iter(&self) -> impl Iterator<Item = EntityId> + '_ {
        self.summary
            .iter()
            .enumerate()
            .flat_map(|(block, &summary)| bits(summary).map(move |bit| block * 64 + bit))
            .flat_map(|word| bits(self.words[word]).map(move |bit| word * 64 + bit))
            .map(|id| EntityId(id as u32))
    }

    /// Appends the ids in ascending order.
    pub(crate) fn extend_into(&self, out: &mut Vec<EntityId>) {
        out.reserve(self.len);
        out.extend(self.iter());
    }
}

fn bits(mut word: u64) -> impl Iterator<Item = usize> {
    std::iter::from_fn(move || {
        (word != 0).then(|| {
            let bit = word.trailing_zeros() as usize;
            word &= word - 1;
            bit
        })
    })
}

#[cfg(test)]
#[path = "id_set_tests.rs"]
mod tests;

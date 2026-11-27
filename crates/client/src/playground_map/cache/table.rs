//! Ordered storage for the bounded source-chunk and prepared-patch caches.

type Coordinate = (i32, i32);

pub(crate) struct ChunkTable<T> {
    keys: Vec<Coordinate>,
    values: Vec<T>,
}

impl<T> Default for ChunkTable<T> {
    fn default() -> Self {
        Self {
            keys: Vec::new(),
            values: Vec::new(),
        }
    }
}

// Share a single search across the caches and keep keys contiguous.
#[inline(never)]
fn locate(keys: &[Coordinate], key: &Coordinate) -> Result<usize, usize> {
    keys.binary_search(key)
}

impl<T> ChunkTable<T> {
    pub(crate) fn len(&self) -> usize {
        self.keys.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    pub(crate) fn clear(&mut self) {
        self.keys.clear();
        self.values.clear();
    }

    pub(crate) fn keys(&self) -> std::slice::Iter<'_, Coordinate> {
        self.keys.iter()
    }

    pub(crate) fn values(&self) -> std::slice::Iter<'_, T> {
        self.values.iter()
    }

    pub(crate) fn values_mut(&mut self) -> std::slice::IterMut<'_, T> {
        self.values.iter_mut()
    }

    pub(crate) fn contains_key(&self, key: &Coordinate) -> bool {
        locate(&self.keys, key).is_ok()
    }

    pub(crate) fn get(&self, key: &Coordinate) -> Option<&T> {
        let index = locate(&self.keys, key).ok()?;
        self.values.get(index)
    }

    pub(crate) fn get_mut(&mut self, key: &Coordinate) -> Option<&mut T> {
        let index = locate(&self.keys, key).ok()?;
        self.values.get_mut(index)
    }

    pub(crate) fn insert(&mut self, key: Coordinate, value: T) {
        match locate(&self.keys, &key) {
            Ok(index) => self.values[index] = value,
            Err(index) => {
                self.keys.insert(index, key);
                self.values.insert(index, value);
            }
        }
    }

    pub(crate) fn remove(&mut self, key: &Coordinate) -> Option<T> {
        let index = locate(&self.keys, key).ok()?;
        self.keys.remove(index);
        Some(self.values.remove(index))
    }
}

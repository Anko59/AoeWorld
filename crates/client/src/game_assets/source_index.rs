//! Source lookups share one index; all mutable access invalidates it.
use std::{
    cell::OnceCell,
    collections::HashMap,
    ops::{Deref, DerefMut},
};

pub(crate) trait FrameIdentity {
    fn source(&self) -> &str;
    fn frame(&self) -> u32;
}

pub(crate) struct IndexedFrames<T> {
    rows: Vec<T>,
    sources: OnceCell<FrameIndex>,
}

impl<T> From<Vec<T>> for IndexedFrames<T> {
    fn from(rows: Vec<T>) -> Self {
        Self {
            rows,
            sources: OnceCell::new(),
        }
    }
}

impl<T> Deref for IndexedFrames<T> {
    type Target = Vec<T>;
    fn deref(&self) -> &Self::Target {
        &self.rows
    }
}

impl<T> DerefMut for IndexedFrames<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.sources.take();
        &mut self.rows
    }
}

impl<T> IntoIterator for IndexedFrames<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;
    fn into_iter(self) -> Self::IntoIter {
        self.rows.into_iter()
    }
}

impl<'a, T> IntoIterator for &'a IndexedFrames<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.rows.iter()
    }
}

impl<'a, T> IntoIterator for &'a mut IndexedFrames<T> {
    type Item = &'a mut T;
    type IntoIter = std::slice::IterMut<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.sources.take();
        self.rows.iter_mut()
    }
}

impl<T: FrameIdentity> IndexedFrames<T> {
    pub(crate) fn sources(&self) -> &FrameIndex {
        self.sources.get_or_init(|| {
            let mut groups = HashMap::<String, Vec<(u32, usize)>>::new();
            for (index, row) in self.rows.iter().enumerate() {
                let value = (row.frame(), index);
                if let Some(group) = groups.get_mut(row.source()) {
                    group.push(value);
                } else {
                    groups.insert(row.source().to_owned(), vec![value]);
                }
            }
            for group in groups.values_mut() {
                group.sort_unstable_by_key(|row| row.0);
            }
            FrameIndex(groups)
        })
    }
}

pub(crate) struct FrameIndex(HashMap<String, Vec<(u32, usize)>>);

impl FrameIndex {
    pub(crate) fn contains(&self, source: &str) -> bool {
        self.0.contains_key(source)
    }

    pub(crate) fn len(&self, source: &str) -> usize {
        self.0.get(source).map_or(0, Vec::len)
    }

    pub(crate) fn prefix(&self, source: &str, count: u32) -> Option<Vec<usize>> {
        if count == 0 {
            return Some(Vec::new());
        }
        let rows = self.0.get(source)?;
        let rows = &rows[..rows.partition_point(|row| row.0 < count)];
        if rows.len() != count as usize
            || rows
                .iter()
                .enumerate()
                .any(|(number, row)| row.0 != number as u32)
        {
            return None;
        }
        Some(rows.iter().map(|row| row.1).collect())
    }
}

#[path = "source_index/tests.rs"]
#[cfg(test)]
mod tests;

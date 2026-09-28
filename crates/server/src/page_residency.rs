use crate::map_store::PageResidency;
use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

pub(crate) const MAX_CACHED_RESIDENCIES: usize = 2;

#[derive(Debug, Default)]
pub(crate) struct Registry {
    providers: BTreeMap<String, Arc<PageResidency>>,
    order: VecDeque<String>,
}

impl Registry {
    pub(crate) fn get(&mut self, hash: &str) -> Option<Arc<PageResidency>> {
        let provider = self.providers.get(hash).cloned()?;
        touch(&mut self.order, hash);
        Some(provider)
    }

    pub(crate) fn insert(&mut self, hash: String, provider: Arc<PageResidency>) {
        self.providers.insert(hash.clone(), provider);
        touch(&mut self.order, &hash);
        while self.providers.len() > MAX_CACHED_RESIDENCIES {
            let Some(evicted) = self.order.pop_front() else {
                break;
            };
            self.providers.remove(&evicted);
        }
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.providers.len()
    }
}

fn touch(order: &mut VecDeque<String>, hash: &str) {
    if let Some(index) = order.iter().position(|candidate| candidate == hash) {
        order.remove(index);
    }
    order.push_back(hash.to_owned());
}

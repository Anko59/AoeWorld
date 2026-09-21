use super::{MAX_PAGE_BYTES, MapStoreError, read_bounded_file};
use aoe_map::{
    ENVIRONMENT_PAGE_SAMPLES, ElevationPage, EnvironmentPage, EnvironmentPageError,
    EnvironmentPageKey, EnvironmentPageProvider, FieldPyramid, HistoricalLandUsePage, MapPackage,
    PageLayer, PageRootBuilder, PotentialBiomePage, WaterPage,
};
use std::{
    collections::{BTreeMap, VecDeque},
    io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

pub(crate) const MAX_RESIDENT_ENVIRONMENT_PAGES: usize = 128;
// 64 MiB admits the supported 16,384-sample pyramid with all four optional
// layers (349,548 records at the conservative fixed-record budget) while
// keeping the metadata separate from the decoded-page payload cache.
const MAX_PAGE_INDEX_BYTES: usize = 64 * 1024 * 1024;
// The index stores one fixed-size key/hash/location record per page. The
// shared root path is accounted for once in PageResidency rather than once
// per entry; this budget also leaves room for BTree node overhead.
const PAGE_INDEX_ENTRY_BYTES: usize = 96;

#[derive(Clone, Debug)]
struct PageEntry {
    expected_hash: [u8; 32],
    location: PageLocation,
}

#[derive(Clone, Copy, Debug)]
enum PageLocation {
    Coordinate,
    Legacy(u32),
}

#[derive(Debug, Default)]
struct PageCache {
    pages: BTreeMap<EnvironmentPageKey, Arc<EnvironmentPage>>,
    order: VecDeque<EnvironmentPageKey>,
}

/// Native runtime adapter for one immutable package. Startup retains only
/// page paths and expected content hashes; decoded pages are bounded by a
/// deterministic LRU and are independently re-verified on every reload.
#[derive(Debug)]
pub(crate) struct PageResidency {
    root: PathBuf,
    entries: BTreeMap<EnvironmentPageKey, PageEntry>,
    cache: Mutex<PageCache>,
}

impl PageResidency {
    pub(crate) fn open(
        directory: &Path,
        package: &MapPackage,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Arc<Self>, MapStoreError> {
        if cancelled() {
            return Err(MapStoreError::Cancelled);
        }
        let mut entries = BTreeMap::new();
        let root = directory.join("pages").join(package.content_hash_hex());
        scan_layer(
            &root,
            PageLayer::Elevation,
            &package.environment.elevation,
            &mut entries,
            cancelled,
        )?;
        if let Some(field) = &package.environment.water {
            scan_layer(&root, PageLayer::Water, field, &mut entries, cancelled)?;
        }
        if let Some(field) = &package.environment.vegetation {
            scan_layer(&root, PageLayer::Vegetation, field, &mut entries, cancelled)?;
        }
        if let Some(field) = &package.environment.historical_land_use {
            scan_layer(
                &root,
                PageLayer::HistoricalLandUse,
                field,
                &mut entries,
                cancelled,
            )?;
        }
        Ok(Arc::new(Self {
            root,
            entries,
            cache: Mutex::new(PageCache::default()),
        }))
    }

    #[cfg(test)]
    pub(crate) fn resident_pages(&self) -> usize {
        self.cache
            .lock()
            .map(|cache| cache.pages.len())
            .unwrap_or(0)
    }

    #[cfg(test)]
    pub(crate) fn indexed_pages(&self) -> usize {
        self.entries.len()
    }
}

impl EnvironmentPageProvider for PageResidency {
    fn page(
        &self,
        key: EnvironmentPageKey,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Arc<EnvironmentPage>, EnvironmentPageError> {
        if cancelled() {
            return Err(EnvironmentPageError::Cancelled);
        }
        if let Ok(mut cache) = self.cache.lock()
            && let Some(page) = cache.pages.get(&key).cloned()
        {
            touch(&mut cache.order, key);
            return Ok(page);
        }
        let entry = self
            .entries
            .get(&key)
            .ok_or(EnvironmentPageError::Missing)?;
        let path = entry.path(&self.root, key);
        let bytes =
            read_bounded_file(&path, MAX_PAGE_BYTES, "environment page").map_err(map_read_error)?;
        if cancelled() {
            return Err(EnvironmentPageError::Cancelled);
        }
        let page = decode_page(key, &bytes).map_err(|_| EnvironmentPageError::Corrupt)?;
        if page
            .content_hash()
            .map_err(|_| EnvironmentPageError::Corrupt)?
            != entry.expected_hash
        {
            return Err(EnvironmentPageError::Corrupt);
        }
        let page = Arc::new(page);
        let mut cache = self
            .cache
            .lock()
            .map_err(|_| EnvironmentPageError::Unavailable)?;
        cache_insert(&mut cache, key, page.clone())?;
        Ok(page)
    }
}

fn cache_insert(
    cache: &mut PageCache,
    key: EnvironmentPageKey,
    page: Arc<EnvironmentPage>,
) -> Result<(), EnvironmentPageError> {
    cache.pages.insert(key, page);
    touch(&mut cache.order, key);
    while cache.pages.len() > MAX_RESIDENT_ENVIRONMENT_PAGES {
        let Some(evicted) = cache.order.pop_front() else {
            return Err(EnvironmentPageError::Unavailable);
        };
        if evicted != key {
            cache.pages.remove(&evicted);
        }
    }
    Ok(())
}

fn scan_layer(
    root: &Path,
    layer: PageLayer,
    field: &FieldPyramid,
    entries: &mut BTreeMap<EnvironmentPageKey, PageEntry>,
    cancelled: &dyn Fn() -> bool,
) -> Result<(), MapStoreError> {
    let directory = root.join(layer.directory_name());
    let mut legacy_index = 0_u32;
    for (level, metadata) in field.levels.iter().enumerate() {
        let side = u16::from(ENVIRONMENT_PAGE_SAMPLES);
        let count = metadata.samples_per_axis.div_ceil(side);
        let mut digest = PageRootBuilder::new(layer, usize::from(count).pow(2))
            .map_err(|error| invalid(&directory, error))?;
        for y in 0..count {
            for x in 0..count {
                if cancelled() {
                    return Err(MapStoreError::Cancelled);
                }
                if entries.len().saturating_add(1) > MAX_PAGE_INDEX_BYTES / PAGE_INDEX_ENTRY_BYTES {
                    return Err(invalid(&directory, "prepared page index exceeds its bound"));
                }
                let coordinate = directory.join(format!("{level}-{x}-{y}.json"));
                let location = if layer == PageLayer::Elevation || coordinate.try_exists()? {
                    PageLocation::Coordinate
                } else {
                    legacy_index = legacy_index.saturating_add(1);
                    PageLocation::Legacy(legacy_index.saturating_sub(1))
                };
                // Legacy pages use the same deterministic row-major numbering
                // as the old reader, including coordinates that happen to be
                // present in the mixed layout.
                if matches!(location, PageLocation::Coordinate) {
                    legacy_index = legacy_index.saturating_add(1);
                }
                let key = EnvironmentPageKey {
                    layer,
                    level: level as u8,
                    x,
                    y,
                };
                let path = match location {
                    PageLocation::Coordinate => coordinate,
                    PageLocation::Legacy(index) => directory.join(format!("{index}.json")),
                };
                let bytes = read_bounded_file(&path, MAX_PAGE_BYTES, "environment page")?;
                let page = decode_page(key, &bytes).map_err(|reason| invalid(&path, reason))?;
                let expected_dimensions = (
                    (metadata.samples_per_axis - x * side).min(side) as u8,
                    (metadata.samples_per_axis - y * side).min(side) as u8,
                );
                if page_dimensions(&page) != expected_dimensions {
                    return Err(invalid(&path, "page dimensions do not match its index"));
                }
                let hash = page.content_hash().map_err(|error| invalid(&path, error))?;
                digest.push(hash).map_err(|error| invalid(&path, error))?;
                if entries
                    .insert(
                        key,
                        PageEntry {
                            expected_hash: hash,
                            location,
                        },
                    )
                    .is_some()
                {
                    return Err(invalid(&directory, "duplicate prepared page coordinate"));
                }
            }
        }
        if digest
            .finish()
            .map_err(|error| invalid(&directory, error))?
            != metadata.ordered_page_root
        {
            return Err(invalid(
                &directory,
                "pages do not reproduce the indexed root",
            ));
        }
    }
    Ok(())
}

impl PageEntry {
    fn path(&self, root: &Path, key: EnvironmentPageKey) -> PathBuf {
        match self.location {
            PageLocation::Coordinate => root
                .join(key.layer.directory_name())
                .join(format!("{}-{}-{}.json", key.level, key.x, key.y)),
            PageLocation::Legacy(index) => root
                .join(key.layer.directory_name())
                .join(format!("{index}.json")),
        }
    }
}

fn decode_page(key: EnvironmentPageKey, bytes: &[u8]) -> Result<EnvironmentPage, &'static str> {
    let page = match key.layer {
        PageLayer::Elevation => {
            serde_json::from_slice::<ElevationPage>(bytes).map(EnvironmentPage::Elevation)
        }
        PageLayer::Water => serde_json::from_slice::<WaterPage>(bytes).map(EnvironmentPage::Water),
        PageLayer::Vegetation => {
            serde_json::from_slice::<PotentialBiomePage>(bytes).map(EnvironmentPage::Vegetation)
        }
        PageLayer::HistoricalLandUse => serde_json::from_slice::<HistoricalLandUsePage>(bytes)
            .map(EnvironmentPage::HistoricalLandUse),
    }
    .map_err(|_| "page JSON is invalid")?;
    if page.key() != key {
        return Err("page coordinates are invalid");
    }
    page.validate().map_err(|_| "page contents are invalid")?;
    Ok(page)
}

fn page_dimensions(page: &EnvironmentPage) -> (u8, u8) {
    match page {
        EnvironmentPage::Elevation(page) => (page.width, page.height),
        EnvironmentPage::Water(page) => (page.width, page.height),
        EnvironmentPage::Vegetation(page) => (page.width, page.height),
        EnvironmentPage::HistoricalLandUse(page) => (page.width, page.height),
    }
}

fn touch(order: &mut VecDeque<EnvironmentPageKey>, key: EnvironmentPageKey) {
    if let Some(index) = order.iter().position(|candidate| *candidate == key) {
        order.remove(index);
    }
    order.push_back(key);
}

fn map_read_error(error: MapStoreError) -> EnvironmentPageError {
    match error {
        MapStoreError::Io(error) if error.kind() == io::ErrorKind::NotFound => {
            EnvironmentPageError::Missing
        }
        MapStoreError::InvalidPackage { .. } | MapStoreError::Json(_) => {
            EnvironmentPageError::Corrupt
        }
        MapStoreError::Io(_) | MapStoreError::TooManyPackages => EnvironmentPageError::Unavailable,
        MapStoreError::Cancelled => EnvironmentPageError::Cancelled,
    }
}

fn invalid(path: &Path, reason: impl std::fmt::Display) -> MapStoreError {
    MapStoreError::InvalidPackage {
        path: path.to_owned(),
        reason: reason.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lru_stays_bounded_under_a_large_valid_page_workload() {
        let mut cache = PageCache::default();
        // This exercises the cache capacity with valid page coordinates.  The
        // large, real-coordinate pyramid eviction fixture lives in
        // `tests::residency`; this unit test only checks the cache itself.
        for index in 0..16_384_u16 {
            let x = index % 256;
            let y = index / 256;
            let key = EnvironmentPageKey {
                layer: PageLayer::Elevation,
                level: 0,
                x,
                y,
            };
            let page = Arc::new(EnvironmentPage::Elevation(ElevationPage {
                level: 0,
                x,
                y,
                width: 64,
                height: 64,
                geographic_height_centimeters: vec![i32::from(index); 64 * 64],
            }));
            cache_insert(&mut cache, key, page).expect("bounded cache insertion");
        }
        assert_eq!(cache.pages.len(), MAX_RESIDENT_ENVIRONMENT_PAGES);
        assert_eq!(cache.order.len(), MAX_RESIDENT_ENVIRONMENT_PAGES);
    }
}

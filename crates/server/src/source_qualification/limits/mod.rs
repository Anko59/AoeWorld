pub(super) const QUALIFICATION_AXIS_TILES: u64 = 50_000;
pub(super) const MAX_ROUTE_TICKS: u64 = 1_200_000;
pub(super) const ORDINARY_ACTIVATION_SEARCH_CHUNKS: usize = 64;
pub(super) const MAX_RESOURCE_SCAN_SIDE: i32 = 64;
pub(super) const MAX_RESIDENT_PAGES: usize = 128;
pub(super) const QUALIFICATION_CASE: &str = "source-backed-100km-50k-tiles-1-to-1";
pub(super) const PARIS_DIAGNOSTIC_CENTER: (i32, i32) = (488_500_000, 20_000_000);
pub(super) const PARIS_DIAGNOSTIC_CHAIN_X: [i32; 6] = [5_000, 7_000, 9_000, 11_000, 13_000, 15_000];
pub(super) const NAVIGATION_CACHE_BYTE_SCOPE: &str =
    "logical payload only; excludes allocator and map container overhead";

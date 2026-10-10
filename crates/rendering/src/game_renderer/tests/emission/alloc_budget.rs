//! Absolute heap budget of one dense scene build on the direct emission path.
use super::*;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static REALLOCS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

struct Counting;
fn grow(n: usize) {
    let live = LIVE.fetch_add(n, Relaxed) + n;
    PEAK.fetch_max(live, Relaxed);
}
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Relaxed);
        BYTES.fetch_add(l.size(), Relaxed);
        grow(l.size());
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        LIVE.fetch_sub(l.size(), Relaxed);
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        REALLOCS.fetch_add(1, Relaxed);
        BYTES.fetch_add(n, Relaxed);
        LIVE.fetch_sub(l.size(), Relaxed);
        grow(n);
        unsafe { System.realloc(p, l, n) }
    }
}
#[global_allocator]
static COUNTING: Counting = Counting;

#[wasm_bindgen::prelude::wasm_bindgen]
extern "C" {
    #[wasm_bindgen::prelude::wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

/// (allocs, reallocs, bytes requested, peak extra live bytes) of `f`.
fn measure<T>(f: impl FnOnce() -> T) -> (usize, usize, usize, usize, T) {
    let base = LIVE.load(Relaxed);
    PEAK.store(base, Relaxed);
    let (a, r, b) = (
        ALLOCS.load(Relaxed),
        REALLOCS.load(Relaxed),
        BYTES.load(Relaxed),
    );
    let out = f();
    let d = |x: &AtomicUsize, s| x.load(Relaxed) - s;
    (
        d(&ALLOCS, a),
        d(&REALLOCS, r),
        d(&BYTES, b),
        PEAK.load(Relaxed) - base,
        out,
    )
}

#[wasm_bindgen_test::wasm_bindgen_test]
fn dense_scene_build_heap_traffic_old_vs_direct() {
    use aoe_core::{EntityId, ScreenPoint};
    let mut art = species_fixture::art();
    let frame = |i| {
        let mut f = species_fixture::frame(i);
        f.atlas.page = (i % 3) as u32;
        f
    };
    art.walking = (0..80).map(frame).collect();
    art.standing = (80..160).map(frame).collect();
    art.grass = vec![frame(160)];
    art.terrain = std::array::from_fn(|i| vec![frame(161 + i)]);
    art.resources[1] = (170..184).map(frame).collect();
    art.tree_shadows = (184..198).map(frame).collect();
    art.tree_families = [
        (198..207).map(frame).collect(),
        (207..220).map(frame).collect(),
    ];
    let mut camera = species_fixture::camera();
    camera.viewport = [1920.0, 1080.0];
    camera.center = [20.0, 20.0];
    let projection = Camera {
        center: camera.center,
        zoom: camera.zoom,
        viewport: camera.viewport,
        focus_elevation_meters: camera.focus_elevation_meters,
    };
    let at = |i: usize, n: usize| {
        let x = (i % n) as f64 / n as f64 * 1900.0 + 10.0;
        let y = (i / n) as f64 / n as f64 * 1060.0 + 10.0;
        projection.screen_to_world_at_height(ScreenPoint { x, y }, 0.0)
    };
    // 4,000 trees (family art, shadows + bodies) and 500 units, 100 selected.
    let resources = (0..4000)
        .map(|i| SceneResource {
            id: i as u64,
            position: at(i, 64),
            kind: 1,
            visual_family: [0, 1, 2, 4][i % 4],
            visual_variant: (i % 9) as u8,
            elevation_meters: 0.0,
        })
        .collect::<Vec<_>>();
    let units = (0..500)
        .map(|i| SceneUnit {
            id: EntityId(i as u32),
            position: at(i * 7 + 3, 23),
            moving: i % 2 == 0,
            facing: (i % 8) as u8,
            selected: i % 5 == 0,
            elevation_meters: 0.0,
        })
        .collect::<Vec<_>>();
    let terrain = (0..1600)
        .map(|i| SceneTerrain {
            appearance: None,
            position: [(i % 40) as f64 + 0.5, (i / 40) as f64 + 0.5],
            material: (i % 5) as u8,
            elevation_meters: 0.0,
            surface: SceneTerrainSurface::flat(0.0),
        })
        .collect::<Vec<_>>();
    let cam = camera;
    let surf = || {
        let mut s = projected_surface_triangles(&terrain, cam);
        apply_terrain_textures(&mut s, &art);
        s
    };
    let s = surf();
    let n_surf = s.len();
    // The first build warms code paths; the second is the measured budget.
    let _ = direct_world_layers(
        surf().into_iter(),
        &art,
        &terrain,
        &resources,
        &units,
        cam,
        0,
    );
    let (allocs, reallocs, bytes, peak, layers) =
        measure(|| direct_world_layers(s.into_iter(), &art, &terrain, &resources, &units, cam, 0));
    let report = format!(
        "layers={} surfaces={n_surf} allocs={allocs} reallocs={reallocs} bytes={bytes} peak_live={peak}",
        layers.len()
    );
    log(&report);
    // The counts are deterministic for this fixture (no margin is applied):
    // one reserved layer vector, one ring vector per selected unit, the sort
    // index/scratch. Raising a bound needs a measured justification.
    assert_eq!(layers.len(), 12_514, "{report}");
    assert_eq!(reallocs, 0, "{report}");
    assert!(allocs <= 102, "{report}");
    assert!(bytes <= 6_592_800, "{report}");
    assert!(peak <= 3_205_440, "{report}");
}

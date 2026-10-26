//! Client presentation policy; it never changes authoritative simulation state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Lod {
    Sprites,
    Markers,
    Map,
}

pub fn lod(zoom: f64) -> Lod {
    // Keep the tile grid readable at the ordinary 1500-delta zoom (~10.54%).
    // The viewport-scaled cutoff still bounds detailed terrain on large canvases.
    if zoom >= 0.1 {
        Lod::Sprites
    } else if zoom >= 0.03125 {
        Lod::Markers
    } else {
        Lod::Map
    }
}

/// Keep the detailed tile area bounded even on large backing canvases.
pub fn lod_for_viewport(zoom: f64, viewport: [f64; 2]) -> Lod {
    lod(zoom / viewport_scale(viewport))
}

fn viewport_scale(viewport: [f64; 2]) -> f64 {
    (viewport[0] * viewport[1] / (1280.0 * 720.0))
        .sqrt()
        .max(1.0)
}

/// A cluster opens the marker view; a marker opens the detailed game view.
pub fn focus_zoom(zoom: f64, viewport: [f64; 2]) -> f64 {
    let target = match lod_for_viewport(zoom, viewport) {
        Lod::Map => 0.0625,
        Lod::Markers => 0.25,
        Lod::Sprites => return zoom,
    };
    (target * viewport_scale(viewport)).min(3.0)
}

//! Barycentric row origins shared by full and clipped rasterization.
use crate::surface_mesh::SurfacePoint;

pub(super) struct RasterPlane {
    pub(super) x: [f64; 2],
    pub(super) y: [f64; 2],
    pub(super) constant: [f64; 2],
}

impl RasterPlane {
    pub(super) fn new(points: [SurfacePoint; 3]) -> Option<Self> {
        let [a, b, c] = [points[0].screen, points[1].screen, points[2].screen];
        let denominator = (b.y - c.y) * (a.x - c.x) + (c.x - b.x) * (a.y - c.y);
        if denominator.abs() < f64::EPSILON {
            return None;
        }
        let x = [(b.y - c.y) / denominator, (c.y - a.y) / denominator];
        let y = [(c.x - b.x) / denominator, (a.x - c.x) / denominator];
        let constant = [-x[0] * c.x - y[0] * c.y, -x[1] * c.x - y[1] * c.y];
        Some(Self { x, y, constant })
    }
}

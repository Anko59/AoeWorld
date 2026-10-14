//! Presented depths choose the hit; cosmetic relief retains source order targets.
use super::*;

pub fn pick_surface_point(
    triangles: &[ProjectedSurfaceTriangle],
    screen: ScreenPoint,
) -> Option<[f64; 2]> {
    let mut best: Option<(f64, [i32; 2], bool, bool, [f64; 2])> = None;
    for triangle in triangles {
        // Faces close the rendered relief but are not order targets. Continue
        // to the nearest eligible ground instead of rejecting the whole ray.
        if triangle.skirt || !triangle.pickable {
            continue;
        }
        let Some(weights) = barycentric(triangle.points, screen) else {
            continue;
        };
        let world = [
            triangle.points[0].world[0] * weights[0]
                + triangle.points[1].world[0] * weights[1]
                + triangle.points[2].world[0] * weights[2],
            triangle.points[0].world[1] * weights[0]
                + triangle.points[1].world[1] * weights[1]
                + triangle.points[2].world[1] * weights[2],
        ];
        let elevation = triangle.points[0].world[2] * weights[0]
            + triangle.points[1].world[2] * weights[1]
            + triangle.points[2].world[2] * weights[2];
        let hit = (
            surface_render_depth([world[0], world[1], elevation], triangle.skirt),
            triangle.tile,
            triangle.skirt,
            triangle.pickable,
            source_point(triangle, weights, world),
        );
        if best.as_ref().is_none_or(|current| {
            hit.0
                .total_cmp(&current.0)
                .then((!hit.2).cmp(&(!current.2)))
                .then(hit.3.cmp(&current.3))
                .then(hit.1.cmp(&current.1))
                .is_gt()
        }) {
            best = Some(hit);
        }
    }
    best.and_then(|(_, _, skirt, pickable, world)| (!skirt && pickable).then_some(world))
}

fn source_point(
    face: &ProjectedSurfaceTriangle,
    weights: [f64; 3],
    presented: [f64; 2],
) -> [f64; 2] {
    let Some(source) = face.source_ground_points() else {
        return presented;
    };

    [0, 1].map(|axis| {
        let value = source
            .iter()
            .zip(weights)
            .map(|(p, w)| p[axis] * w)
            .sum::<f64>();
        let lo = source.iter().map(|p| p[axis]).fold(f64::INFINITY, f64::min);
        let hi = source
            .iter()
            .map(|p| p[axis])
            .fold(f64::NEG_INFINITY, f64::max);
        value.clamp(lo + 0.000001, hi - 0.000001)
    })
}

impl ProjectedSurfaceTriangle {
    /// Eligible ground before cosmetic cliff rim movement. Contacts stay on
    /// these source coordinates; depth and rendering use the displayed points.
    pub fn source_ground_points(&self) -> Option<[[f64; 3]; 3]> {
        if self.skirt || !self.pickable || self.order & SCULPTED_ORDER == 0 {
            return None;
        }
        let corners = |mode| match mode {
            0 => [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]],
            1 => [[0.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            2 => [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
            _ => [[1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        };
        let at = self.texture_tile.map(f64::from);
        let world =
            |mode, step: f64| corners(mode).map(|p| [at[0] + p[0] * step, at[1] + p[1] * step]);
        let source = world(
            self.order & 3,
            f64::from(1_u32 << ((self.order & 127) >> 2)),
        );
        Some(std::array::from_fn(|i| {
            [source[i][0], source[i][1], self.points[i].world[2]]
        }))
    }
}

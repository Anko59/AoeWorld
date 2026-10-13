//! Closed terrain edges, sharing the exact displayed ground sheets.
use super::*;

#[derive(Clone, Copy)]
pub(super) enum Edge {
    East,
    South,
}

#[derive(Clone, Copy)]
pub(super) struct SkirtSurface {
    pub(super) sample: SceneTerrain,
    pub(super) heights: [f64; 4],
}

pub(super) fn append_edge_skirt(
    result: &mut Vec<ProjectedSurfaceTriangle>,
    projection: &Camera,
    tile: [i32; 2],
    size: i32,
    current: SkirtSurface,
    neighbor: Option<SkirtSurface>,
    edge: Edge,
) {
    let Some(neighbor) = neighbor else {
        return;
    };
    let sample = current.sample;
    let adjacent_sample = neighbor.sample;
    if sample.surface.water == 0
        && adjacent_sample.surface.water == 0
        && !sample.surface.is_cliff()
        && !adjacent_sample.surface.is_cliff()
    {
        return;
    }
    let (indices, adjacent_indices) = match edge {
        Edge::East => ([1, 2], [0, 3]),
        Edge::South => ([2, 3], [1, 0]),
    };
    let a = indices.map(|i| current.heights[i]);
    let b = adjacent_indices.map(|i| neighbor.heights[i]);
    let difference = [a[0] - b[0], a[1] - b[1]];
    if difference == [0.0; 2] {
        return;
    }
    let xy = match edge {
        Edge::East => [
            [f64::from(tile[0] + size), f64::from(tile[1])],
            [f64::from(tile[0] + size), f64::from(tile[1] + size)],
        ],
        Edge::South => [
            [f64::from(tile[0] + size), f64::from(tile[1] + size)],
            [f64::from(tile[0]), f64::from(tile[1] + size)],
        ],
    };
    let top = [a[0].max(b[0]), a[1].max(b[1])];
    let bottom = [a[0].min(b[0]), a[1].min(b[1])];
    let points = [
        project_world(projection, xy[0], top[0]),
        project_world(projection, xy[1], top[1]),
        project_world(projection, xy[1], bottom[1]),
        project_world(projection, xy[0], bottom[0]),
    ];
    let order = match edge {
        Edge::East => 2,
        Edge::South => 4,
    };
    if difference[0] * difference[1] < 0.0 {
        // Endpoint extrema cannot be interpolated across a sheet crossing:
        // that invents a floating upper edge and a buried lower edge. Each
        // side is one triangle terminating at the sheets' true intersection.
        let t = difference[0] / (difference[0] - difference[1]);
        let crossing = project_world(
            projection,
            [
                xy[0][0] + (xy[1][0] - xy[0][0]) * t,
                xy[0][1] + (xy[1][1] - xy[0][1]) * t,
            ],
            a[0] + (a[1] - a[0]) * t,
        );
        result.push(triangle(
            [points[0], crossing, points[3]],
            tile,
            paint(
                sample,
                adjacent_sample,
                difference[0] > 0.0,
                top[0] - bottom[0],
            ),
            5,
            order,
        ));
        result.push(triangle(
            [crossing, points[1], points[2]],
            tile,
            paint(
                sample,
                adjacent_sample,
                difference[1] > 0.0,
                top[1] - bottom[1],
            ),
            4,
            order + 1,
        ));
    } else {
        let paint = paint(
            sample,
            adjacent_sample,
            difference[0] + difference[1] > 0.0,
            (top[0] - bottom[0]).max(top[1] - bottom[1]),
        );
        result.push(triangle(
            [points[0], points[1], points[2]],
            tile,
            paint,
            4,
            order,
        ));
        result.push(triangle(
            [points[0], points[2], points[3]],
            tile,
            paint,
            5,
            order + 1,
        ));
    }
}

#[derive(Clone, Copy)]
struct Paint {
    bank: SceneTerrain,
    material: u8,
    tint: u8,
    neutral: bool,
}

fn paint(sample: SceneTerrain, adjacent: SceneTerrain, current_upper: bool, rise: f64) -> Paint {
    let shore = (sample.surface.water != 0) != (adjacent.surface.water != 0);
    let water_step = sample.surface.water != 0 && adjacent.surface.water != 0;
    let upper = if current_upper { sample } else { adjacent };
    let cliff = sample.surface.water == 0
        && adjacent.surface.water == 0
        && (sample.surface.is_cliff() || adjacent.surface.is_cliff());
    // A lowered contour can meet uphill ground. Its closing strip belongs to
    // that ground sheet; it is not an exposed cliff face or a new rock wedge.
    let ground_seam = cliff && !upper.surface.is_cliff();
    let bank = if ground_seam {
        upper
    } else if sample.surface.water != 0 && adjacent.surface.water == 0 {
        adjacent
    } else {
        sample
    };
    let material = if water_step {
        5
    } else if ground_seam || (shore && !bank.surface.is_cliff()) {
        bank.material
    } else {
        4
    };
    let neutral = ground_seam || (shore && material != 4 && rise <= 1.0);
    let tint = if cliff && !ground_seam {
        13
    } else if water_step || neutral {
        0
    } else {
        3
    };
    Paint {
        bank,
        material,
        tint,
        neutral,
    }
}

fn triangle(
    points: [SurfacePoint; 3],
    tile: [i32; 2],
    paint: Paint,
    texture_mode: u8,
    order: u8,
) -> ProjectedSurfaceTriangle {
    ProjectedSurfaceTriangle {
        appearance: landscape::pack(paint.bank.appearance),
        floor_strengths: None,
        mix: [0; 6],
        points,
        color: darken(
            surface_color(paint.bank),
            if paint.neutral { 1.0 } else { 0.62 },
        ),
        tile,
        skirt: true,
        material: paint.material,
        texture_mode,
        tint: paint.tint,
        texture_uv: None,
        texture_blend: None,
        texture_tile: tile,
        texture_materials: paint.neutral.then_some([paint.material; 3]),
        pickable: false,
        order,
    }
}

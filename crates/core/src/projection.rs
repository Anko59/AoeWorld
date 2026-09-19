use crate::{TileCoord, TileRect, WorldConfig};

pub const ISO_TILE_WIDTH: f64 = 128.0;
pub const ISO_TILE_HEIGHT: f64 = 64.0;
pub const ISO_ELEVATION_METER_HEIGHT: f64 = ISO_TILE_HEIGHT / 2.0;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScreenPoint {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub center: [f64; 2],
    pub zoom: f64,
    pub viewport: [f64; 2],
}

impl Camera {
    pub fn new(center: [f64; 2], viewport: [f64; 2]) -> Self {
        Self {
            center,
            zoom: 1.0,
            viewport,
        }
    }

    pub fn world_to_screen(self, world: [f64; 2]) -> ScreenPoint {
        let projected = project(world);
        let camera = project(self.center);
        ScreenPoint {
            x: (self.viewport[0] * 0.5) + (projected.x - camera.x) * self.zoom,
            y: (self.viewport[1] * 0.5) + (projected.y - camera.y) * self.zoom,
        }
    }

    pub fn world_to_screen_at_height(self, world: [f64; 2], elevation_meters: f64) -> ScreenPoint {
        let mut screen = self.world_to_screen(world);
        screen.y -= elevation_meters * ISO_ELEVATION_METER_HEIGHT * self.zoom;
        screen
    }

    pub fn screen_to_world(self, screen: ScreenPoint) -> [f64; 2] {
        let camera = project(self.center);
        let projected = ScreenPoint {
            x: camera.x + (screen.x - self.viewport[0] * 0.5) / self.zoom,
            y: camera.y + (screen.y - self.viewport[1] * 0.5) / self.zoom,
        };
        inverse_project(projected)
    }

    pub fn screen_to_world_at_height(
        self,
        mut screen: ScreenPoint,
        elevation_meters: f64,
    ) -> [f64; 2] {
        screen.y += elevation_meters * ISO_ELEVATION_METER_HEIGHT * self.zoom;
        self.screen_to_world(screen)
    }

    pub fn zoom_around(mut self, pointer: ScreenPoint, zoom: f64) -> Self {
        let before = self.screen_to_world(pointer);
        self.zoom = zoom.clamp(0.25, 3.0);
        let after = self.screen_to_world(pointer);
        self.center[0] += before[0] - after[0];
        self.center[1] += before[1] - after[1];
        self
    }

    pub fn clamp_center(mut self, config: WorldConfig) -> Self {
        let corners = [
            self.screen_to_world(ScreenPoint { x: 0.0, y: 0.0 }),
            self.screen_to_world(ScreenPoint {
                x: self.viewport[0],
                y: self.viewport[1],
            }),
        ];
        let min_x = corners
            .iter()
            .map(|point| point[0])
            .fold(f64::INFINITY, f64::min);
        let max_x = corners
            .iter()
            .map(|point| point[0])
            .fold(f64::NEG_INFINITY, f64::max);
        let min_y = corners
            .iter()
            .map(|point| point[1])
            .fold(f64::INFINITY, f64::min);
        let max_y = corners
            .iter()
            .map(|point| point[1])
            .fold(f64::NEG_INFINITY, f64::max);
        let width = f64::from(config.width_tiles);
        let height = f64::from(config.height_tiles);
        if min_x < 0.0 {
            self.center[0] -= min_x;
        }
        if max_x > width {
            self.center[0] -= max_x - width;
        }
        if min_y < 0.0 {
            self.center[1] -= min_y;
        }
        if max_y > height {
            self.center[1] -= max_y - height;
        }
        self.center[0] = self.center[0].clamp(0.0, width);
        self.center[1] = self.center[1].clamp(0.0, height);
        self
    }

    pub fn visible_tiles(self, config: WorldConfig, prefetch_tiles: f64) -> TileRect {
        let points = [
            ScreenPoint { x: 0.0, y: 0.0 },
            ScreenPoint {
                x: self.viewport[0],
                y: 0.0,
            },
            ScreenPoint {
                x: 0.0,
                y: self.viewport[1],
            },
            ScreenPoint {
                x: self.viewport[0],
                y: self.viewport[1],
            },
        ];
        let worlds = points.map(|point| self.screen_to_world(point));
        let min_x = worlds
            .iter()
            .map(|point| point[0])
            .fold(f64::INFINITY, f64::min)
            - prefetch_tiles;
        let max_x = worlds
            .iter()
            .map(|point| point[0])
            .fold(f64::NEG_INFINITY, f64::max)
            + prefetch_tiles;
        let min_y = worlds
            .iter()
            .map(|point| point[1])
            .fold(f64::INFINITY, f64::min)
            - prefetch_tiles;
        let max_y = worlds
            .iter()
            .map(|point| point[1])
            .fold(f64::NEG_INFINITY, f64::max)
            + prefetch_tiles;
        TileRect::new(
            TileCoord::new(min_x.floor() as i32, min_y.floor() as i32),
            TileCoord::new(max_x.ceil() as i32 + 1, max_y.ceil() as i32 + 1),
        )
        .clamp(config.width_tiles, config.height_tiles)
    }
}

pub fn project(world: [f64; 2]) -> ScreenPoint {
    ScreenPoint {
        x: (world[0] - world[1]) * (ISO_TILE_WIDTH / 2.0),
        y: (world[0] + world[1]) * (ISO_TILE_HEIGHT / 2.0),
    }
}

pub fn inverse_project(projected: ScreenPoint) -> [f64; 2] {
    [
        projected.x / ISO_TILE_WIDTH + projected.y / ISO_TILE_HEIGHT,
        -projected.x / ISO_TILE_WIDTH + projected.y / ISO_TILE_HEIGHT,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Seed;

    #[test]
    fn projection_round_trips_far_coordinates() {
        for world in [
            [0.0, 0.0],
            [1.0, 1.0],
            [16_383.75, 0.25],
            [16_384.0, 16_384.0],
        ] {
            let actual = inverse_project(project(world));
            assert!((actual[0] - world[0]).abs() < 1e-9);
            assert!((actual[1] - world[1]).abs() < 1e-9);
        }
    }

    #[test]
    fn camera_zoom_keeps_cursor_world_point_fixed() {
        let camera = Camera {
            center: [500.0, 700.0],
            zoom: 1.0,
            viewport: [1280.0, 720.0],
        };
        let pointer = ScreenPoint { x: 897.0, y: 251.0 };
        let world = camera.screen_to_world(pointer);
        let zoomed = camera.zoom_around(pointer, 2.0);
        let after = zoomed.screen_to_world(pointer);
        assert!((world[0] - after[0]).abs() < 1e-9);
        assert!((world[1] - after[1]).abs() < 1e-9);
    }

    #[test]
    fn visible_tiles_are_outward_rounded_and_clamped() {
        let config = WorldConfig::new(100, 80, Seed(1)).unwrap();
        let camera = Camera {
            center: [0.0, 0.0],
            zoom: 1.0,
            viewport: [256.0, 128.0],
        };
        let rect = camera.visible_tiles(config, 2.0);
        assert_eq!(rect.min, TileCoord::new(0, 0));
        assert!(rect.max.x <= 100 && rect.max.y <= 80);
    }
}

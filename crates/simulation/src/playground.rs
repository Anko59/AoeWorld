//! Fixed-step, integer movement for the first playable map.
use aoe_core::Position;

pub const WIDTH: i32 = 960;
pub const HEIGHT: i32 = 640;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Playground {
    pub position: Position,
    pub destination: Position,
}

impl Default for Playground {
    fn default() -> Self {
        let position = Position { x: 480, y: 320 };
        Self {
            position,
            destination: position,
        }
    }
}

impl Playground {
    pub fn move_to(&mut self, destination: Position) {
        self.destination = Position {
            x: destination.x.clamp(20, WIDTH - 20),
            y: destination.y.clamp(28, HEIGHT - 20),
        };
    }

    pub fn moving(&self) -> bool {
        self.position != self.destination
    }

    /// Advance 20 ms. Integer normalization bounds speed and prevents overshoot.
    pub fn advance(&mut self) {
        let dx = self.destination.x - self.position.x;
        let dy = self.destination.y - self.position.y;
        let distance = ((i64::from(dx).pow(2) + i64::from(dy).pow(2)) as u64).isqrt() as i32;
        if distance <= 3 {
            self.position = self.destination;
        } else {
            self.position.x += dx * 3 / distance;
            self.position.y += dy * 3 / distance;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrives_without_overshoot_in_every_direction() {
        for x in [20, 479, 480, 481, 940] {
            for y in [28, 319, 320, 321, 620] {
                let mut world = Playground::default();
                let target = Position { x, y };
                world.move_to(target);
                for _ in 0..600 {
                    world.advance();
                }
                assert_eq!(world.position, target);
                assert!(!world.moving());
                world.advance();
                assert_eq!(world.position, target);
            }
        }
    }

    #[test]
    fn commands_clamp_and_replay_deterministically() {
        let mut a = Playground::default();
        let mut b = a.clone();
        for target in [
            Position {
                x: i32::MAX,
                y: i32::MIN,
            },
            Position { x: 80, y: 600 },
        ] {
            a.move_to(target);
            b.move_to(target);
            for _ in 0..70 {
                a.advance();
                b.advance();
                assert_eq!(a, b);
            }
        }
        assert!(a.position.x >= 20 && a.position.x <= WIDTH - 20);
        assert!(a.position.y >= 28 && a.position.y <= HEIGHT - 20);
        assert!(a.moving());
    }
}

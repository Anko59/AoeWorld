//! Native frame addressing and sampling.
use super::{BOLT_SPRITE, native, projectile};

#[derive(Clone, Copy)]
pub struct ActorSource {
    pub id: u32,
    pub stride: u8,
    /// Rounded milliseconds per native cell, not one universal animation rate.
    pub millis: u16,
    /// Upper bound on selected native cells per facing; zero uses the base policy.
    pub phases: u8,
    native: Option<native::Graphic>,
}
impl ActorSource {
    #[cfg(test)]
    pub(in crate::catalog) fn native_stride(self) -> usize {
        self.native.map_or_else(
            || self.frames() as usize / self.rows(),
            |graphic| usize::from(graphic.stride),
        )
    }

    pub const fn new(id: u32, stride: u8, millis: u16, phases: u8) -> Self {
        Self {
            id,
            stride,
            millis,
            phases,
            native: native::get(id),
        }
    }
    pub const fn is_native(self) -> bool {
        self.native.is_some() || self.id == 4500 || self.id == BOLT_SPRITE
    }
    /// Native DE durations retain fractional milliseconds (DAT f32 seconds).
    pub fn frame_millis(self) -> f64 {
        if let Some(graphic) = self.native {
            return if self.stride == 1 && graphic.stride > 1 {
                0.0
            } else {
                f64::from(graphic.seconds) * 1000.0
            };
        }
        if self.id == 4500 {
            return f64::from(1.0_f32 / 60.0) * 1000.0;
        }
        f64::from(self.millis)
    }
    pub const fn replay_delay_seconds(self) -> f32 {
        match self.native {
            Some(g) => g.replay_delay,
            None => 0.0,
        }
    }
    pub const fn sample_step(self) -> u8 {
        if self.native.is_some() {
            return 1;
        }
        if self.phases > 0 {
            return self.stride.div_ceil(self.phases);
        }
        if self.stride >= 8 && self.id != 990 {
            2
        } else {
            1
        }
    }
    pub const fn frames(self) -> u32 {
        if let Some(g) = self.native {
            return g.cells as u32;
        }
        match self.id {
            4500 => 30,
            BOLT_SPRITE => 32,
            3799 => 176,
            3820 => 170,
            _ => self.stride as u32 * 5,
        }
    }
    pub const fn rows(self) -> usize {
        if let Some(g) = self.native {
            g.angles as usize
        } else if self.id == 4500 {
            1
        } else {
            5
        }
    }
    pub const fn selected_frames(self) -> usize {
        self.stride.div_ceil(self.sample_step()) as usize * self.rows()
    }
    pub const fn source_frame(self, row: usize, phase: usize) -> usize {
        if let Some(g) = self.native {
            return row * g.stride as usize
                + if self.stride == 1 && g.stride > 1 {
                    g.stride as usize - 1
                } else {
                    phase
                };
        }
        match self.id {
            4500 => phase,
            BOLT_SPRITE => projectile::BOLT_CELLS[row],
            3799 | 3820 => projectile::source_cell(self.id, row),
            _ => row * self.stride as usize + phase,
        }
    }
    pub fn manifest_source(self) -> String {
        if let Some(g) = self.native {
            return format!("de/{}.sld", g.stem);
        }
        if self.id == 4500 {
            return "de/p_mangonel_x1.sld".to_owned();
        }
        if self.id == BOLT_SPRITE {
            return "de/p_bolt_x1.sld".to_owned();
        }
        format!("graphics.drs:[32, 112, 108, 115]:{}", self.id)
    }
}
pub(super) const fn sprite(id: u32, stride: u8, millis: u16) -> ActorSource {
    ActorSource::new(id, stride, millis, 0)
}

use aoe_protocol::FogState;

/// One bounded, authoritative subscription view. Terrain residency is separate.
#[derive(Default)]
pub struct Fog {
    state: Option<FogState>,
}

impl Fog {
    pub fn apply(&mut self, state: FogState) -> bool {
        if !state.valid() {
            return false;
        }
        let changed = self.state.as_ref().is_none_or(|old| {
            old.disabled != state.disabled || old.region != state.region || old.tiles != state.tiles
        });
        self.state = Some(state);
        changed
    }
    pub fn clear(&mut self) {
        self.state = None;
    }
    pub fn disabled(&self) -> bool {
        self.state.as_ref().is_some_and(|s| s.disabled)
    }
    pub fn state(&self, tile: [i32; 2]) -> u8 {
        let Some(state) = &self.state else {
            return 0;
        };
        if state.disabled {
            return 2;
        }
        let [x, y] = tile;
        let r = state.region;
        if x < r.min.x || y < r.min.y || x >= r.max.x || y >= r.max.y {
            return 0;
        }
        state.tiles[((y - r.min.y) * r.width() + x - r.min.x) as usize]
    }
    pub fn brightness(&self, world: [f64; 2]) -> f32 {
        if self.disabled() {
            return 1.0;
        }
        let [x, y] = world.map(|v| v - 0.5);
        let [tx, ty] = [x.floor() as i32, y.floor() as i32];
        let [fx, fy] = [(x - x.floor()) as f32, (y - y.floor()) as f32];
        let light = |x, y| match self.state([x, y]) {
            0 => 0.0,
            1 => 0.45,
            _ => 1.0,
        };
        let top = light(tx, ty) * (1.0 - fx) + light(tx + 1, ty) * fx;
        let bottom = light(tx, ty + 1) * (1.0 - fx) + light(tx + 1, ty + 1) * fx;
        top * (1.0 - fy) + bottom * fy
    }
}

#[cfg(test)]
mod tests;

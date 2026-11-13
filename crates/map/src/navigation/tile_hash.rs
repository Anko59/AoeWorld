//! Integer-key hashing shared by navigation records and derived ground.
use std::hash::Hasher;

/// A multiply-rotate hasher for small integer keys (FxHash's mixing step).
#[derive(Clone, Default)]
pub(super) struct TileHasher(u64);

impl Hasher for TileHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.write_u64(u64::from(*byte));
        }
    }

    fn write_u64(&mut self, value: u64) {
        self.0 = (self.0.rotate_left(5) ^ value).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
    }

    fn write_i32(&mut self, value: i32) {
        self.write_u64(u64::from(value as u32));
    }

    fn write_u8(&mut self, value: u8) {
        self.write_u64(u64::from(value));
    }
}

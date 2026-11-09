//! Land radius classes and packed minimum-clearance tags. A tag records the
//! largest bucket an edge admits: zero is blocked, then small, medium, large.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum LandBucket {
    Small,
    Medium,
    Large,
}

impl LandBucket {
    pub const ALL: [Self; 3] = [Self::Small, Self::Medium, Self::Large];

    pub fn for_radius(radius: i32) -> Option<Self> {
        match radius {
            0..=256 => Some(Self::Small),
            257..=410 => Some(Self::Medium),
            411..=461 => Some(Self::Large),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub(super) struct BucketEdges(u16);

impl BucketEdges {
    pub(super) const ALL: Self = Self(u16::MAX);

    pub(super) fn set(&mut self, direction: usize, bucket: Option<LandBucket>) {
        let shift = direction * 2;
        self.0 = (self.0 & !(3 << shift)) | (bucket.map_or(0, |bucket| bucket as u16 + 1) << shift);
    }

    pub(super) fn clearance(self, direction: usize) -> Option<LandBucket> {
        match (self.0 >> (direction * 2)) & 3 {
            1 => Some(LandBucket::Small),
            2 => Some(LandBucket::Medium),
            3 => Some(LandBucket::Large),
            _ => None,
        }
    }

    pub(super) fn allows(self, direction: usize, bucket: LandBucket) -> bool {
        self.clearance(direction)
            .is_some_and(|limit| bucket <= limit)
    }
}

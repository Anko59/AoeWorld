use serde::{Deserialize, Serialize};

/// One bounded military group. Larger armies can use several groups.
pub const MAX_FORMATION_MEMBERS: usize = 4_096;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum Formation {
    #[default]
    Line,
    Box,
    Staggered,
    Flank,
}

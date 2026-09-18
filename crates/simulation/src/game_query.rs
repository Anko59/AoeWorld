#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GameQueryStats {
    pub visited_chunks: u32,
    pub candidate_units: u32,
    pub returned_units: u32,
}

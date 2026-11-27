use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StartRequest {
    #[serde(default)]
    pub(super) map_hash: String,
    #[serde(default)]
    pub(super) opponents: usize,
    #[serde(default)]
    pub(super) relic_victory: bool,
    /// Resume this saved game instead of placing new villages.
    #[serde(default)]
    pub(super) save: Option<String>,
}

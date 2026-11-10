//! Owner-visible explanation of what a Steward's domain is doing, derived
//! from the same facts the economy pass acts on. Telemetry only: it never
//! influences a decision, so it is not part of the canonical hash.

/// Why a domain is not growing, in the order the economy pass checks them.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum StewardWait {
    /// Not yet evaluated, or training is under way.
    #[default]
    Growing,
    /// A budgeted residents/buildings census is still in progress. Production
    /// resumes after its retained scan reaches the end.
    Census,
    /// Residents plus queued villagers reached the 100-villager limit.
    Full,
    /// No complete Town Center lies inside this Steward's territory.
    NoTownCenter,
    /// The player's population cap is reached; a House is wanted.
    Population,
    /// Not enough food for the next villager.
    Food,
    /// The owner has switched villager training off for this Steward.
    Paused,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StewardStatus {
    pub settings: super::StewardSettings,
    /// Villagers this Steward currently manages.
    pub managed: u16,
    /// Living villagers (including sheltered) inside its territory.
    pub residents: u16,
    pub wait: StewardWait,
}

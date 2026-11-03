//! Owner-visible explanation of what a Vassal's domain is doing, derived
//! from the same facts the economy pass acts on. Telemetry only: it never
//! influences a decision, so it is not part of the canonical hash.

/// Why a domain is not growing, in the order the economy pass checks them.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum VassalWait {
    /// Not yet evaluated, or training is under way.
    #[default]
    Growing,
    /// The unit census is incomplete (too many units nearby), so nothing is
    /// trained or built until a full scan succeeds.
    Census,
    /// Residents plus queued villagers reached the 100-villager limit.
    Full,
    /// No complete Town Center lies inside this Vassal's territory.
    NoTownCenter,
    /// The player's population cap is reached; a House is wanted.
    Population,
    /// Not enough food for the next villager.
    Food,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct VassalStatus {
    /// Villagers this Vassal currently manages.
    pub managed: u16,
    /// Living villagers (including sheltered) inside its territory.
    pub residents: u16,
    pub wait: VassalWait,
}

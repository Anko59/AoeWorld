//! What a restore spends its time on, told to the caller that has a clock.

/// A part of a restore, reported to `restore_snapshot_with_probe`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RestorePhase {
    /// The map's resource changes.
    Resources,
    /// The tiles each player has explored.
    Explored,
    Buildings,
    /// Placing the units, with the sheltered ones put into their building. The
    /// embarked ones are placed here too, but board their ship in `Rest`.
    Units,
    /// Last sightings, and what every player sees once the fog is back. Also
    /// the players' second copy, made after `JobsAndOrders`.
    FogAndVision,
    /// The orders given again, with the route planning they ask for.
    JobsAndOrders,
    /// Everything else, up to the end of the restore: the players' first copy
    /// and the boarding of embarked units are here.
    Rest,
}

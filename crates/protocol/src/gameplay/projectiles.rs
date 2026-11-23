use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ProjectileKind {
    Arrow,
    Javelin,
    Stone,
    Bolt,
    /// Primary projectile identity from the unit data.
    Native(u16),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProjectileState {
    pub id: u64,
    /// The shooter, only while its firing position is visible to the viewer.
    pub attacker: Option<EntityId>,
    pub from: WorldPosition,
    pub to: WorldPosition,
    pub launched: Tick,
    pub impact: Tick,
    pub kind: ProjectileKind,
}
pub(super) fn valid_projectile(p: &ProjectileState, tick: Tick) -> bool {
    p.kind != ProjectileKind::Native(0)
        && p.launched <= tick
        && tick < p.impact
        && p.impact > p.launched
        && p.impact.0 - p.launched.0 <= 10000
        && [p.from.x, p.from.y, p.to.x, p.to.y]
            .into_iter()
            .all(|v| v >= 0)
}

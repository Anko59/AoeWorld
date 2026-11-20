//! Primary projectile ids and speeds from the installed DE DAT. Art lookup can
//! use the projectile identity even when the shooter is hidden or has died.
use super::ProjectileKind;
use crate::UnitKind;

pub struct DatProjectile {
    pub id: u16,
    pub speed_hundredths: u16,
    pub(crate) kind: ProjectileKind,
}

pub const fn dat_projectile(unit: UnitKind) -> Option<DatProjectile> {
    use ProjectileKind::*;
    use UnitKind::*;
    let (id, speed_hundredths, kind) = match unit {
        CavalryArcher => (477, 700, Arrow),
        HeavyCavalryArcher => (478, 700, Arrow),
        HandCannoneer => (380, 750, Arrow),
        Scorpion => (367, 600, Bolt),
        HeavyScorpion => (627, 600, Bolt),
        Mangonel | Onager | SiegeOnager => (656, 350, Stone),
        BombardCannon => (368, 400, Stone),
        TrebuchetUnpacked => (371, 350, Stone),
        Galley => (512, 600, Arrow),
        WarGalley => (372, 600, Bolt),
        Galleon => (373, 600, Bolt),
        FireGalley | FireShip | FastFireShip => (676, 300, Melee),
        Hulk | WarHulk | Carrack => (2636, 300, Melee),
        CannonGalleon | EliteCannonGalleon => (374, 350, Stone),
        CatapultGalleon => (1932, 375, Stone),
        _ => return None,
    };
    Some(DatProjectile {
        id,
        speed_hundredths,
        kind,
    })
}

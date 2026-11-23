//! Retain the last two exact inputs for an office. Automatic gates can toggle
//! while units pass, or change twice before this office's next service phase.
use super::{
    Claim, Field, GameWorld, PatchBlocks, PlayerId, TileRect, ZoneCache, field::TerrainKey,
};
use std::sync::Arc;

#[derive(Debug)]
pub(super) struct OfficeField {
    revision: u64,
    pub(super) active: Variant,
    alternate: Option<Variant>,
}

#[derive(Debug, Clone)]
pub(super) struct Variant {
    pub(super) field: Arc<Field>,
    pub(super) claims: Arc<[Claim]>,
    patch_revision: u64,
}

impl OfficeField {
    pub(super) fn refresh(
        world: &GameWorld,
        player: PlayerId,
        origin: TileRect,
        previous: Option<&Self>,
        blocks: &mut Option<PatchBlocks<'_>>,
    ) -> Option<Self> {
        let cache = &world.economy.walking;
        let bounds = Field::bounds(world, origin);
        let revision = cache.latest(bounds);
        let mut reused = previous
            .filter(|old| old.revision == revision && old.active.field.terrain.origin == origin)
            .map(|old| &old.active);
        let field = if let Some(reused) = reused {
            reused.field.clone()
        } else {
            let terrain = TerrainKey::new(world, player, origin, bounds);
            reused = previous.and_then(|old| {
                std::iter::once(&old.active)
                    .chain(&old.alternate)
                    .find(|v| v.field.terrain == terrain)
            });
            if let Some(reused) = reused {
                reused.field.clone()
            } else if let Some(old) = previous {
                Arc::new(Field::repaired(world, terrain, &old.active.field)?)
            } else {
                Arc::new(Field::new(world, terrain)?)
            }
        };
        let claims = reused
            .filter(|v| v.patch_revision == cache.patch_revision)
            .map_or_else(
                || {
                    blocks
                        .as_mut()
                        .map_or_else(Arc::default, |store| store.claims(&field))
                },
                |v| v.claims.clone(),
            );
        let alternate = previous.and_then(|old| {
            if Arc::ptr_eq(&old.active.field, &field) {
                old.alternate.clone()
            } else {
                Some(old.active.clone())
            }
        });
        Some(Self {
            revision,
            active: Variant {
                field,
                claims,
                patch_revision: cache.patch_revision,
            },
            alternate,
        })
    }

    pub(super) fn current(&self, cache: &ZoneCache) -> bool {
        self.revision == cache.latest(self.active.field.terrain.bounds)
    }
}

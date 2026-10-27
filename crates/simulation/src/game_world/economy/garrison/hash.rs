use super::*;

fn occupants(hash: &mut blake3::Hasher, units: &[GarrisonedUnit]) {
    hash.update(&(units.len() as u64).to_le_bytes());
    for g in units {
        hash.update(&g.unit.id.0.to_le_bytes());
        hash.update(&g.unit.player.0.to_le_bytes());
        hash.update(&[g.unit.kind as u8]);
        hash.update(&[g.unit.stance as u8]);
        hash.update(&g.unit.hp.to_le_bytes());
        hash.update(&g.unit.max_hp.to_le_bytes());
        hash.update(&g.next_hit_tick.to_le_bytes());
        hash.update(&g.heal_at.to_le_bytes());
        crate::game_world::combat::hash_hit(hash, g.unit.last_hit);
        hash.update(&[g.unit.carried.map_or(u8::MAX, |c| c.kind as u8)]);
        hash.update(&g.unit.carried.map_or(0, |c| c.amount).to_le_bytes());
    }
}

impl super::super::Economy {
    pub(crate) fn hash_garrisons(&self, hash: &mut blake3::Hasher) {
        // Sparse state has a domain marker; empty shelter state adds no work.
        if self.garrison_orders.is_empty()
            && self.occupied_garrisons.is_empty()
            && self.ejections.is_empty()
        {
            return;
        }
        hash.update(b"garrisons");
        hash.update(&(self.garrison_orders.len() as u64).to_le_bytes());
        for (id, building) in &self.garrison_orders {
            hash.update(&id.0.to_le_bytes());
            hash.update(&building.0.to_le_bytes());
        }
        hash.update(&(self.occupied_garrisons.len() as u64).to_le_bytes());
        for id in &self.occupied_garrisons {
            hash.update(&id.0.to_le_bytes());
            if let Some(b) = self.buildings.get(id) {
                occupants(hash, &b.garrison);
            }
        }
        hash.update(&(self.ejections.len() as u64).to_le_bytes());
        for (id, (rect, units)) in &self.ejections {
            hash.update(&id.0.to_le_bytes());
            for value in [rect.min.x, rect.min.y, rect.max.x, rect.max.y] {
                hash.update(&value.to_le_bytes());
            }
            occupants(hash, units);
        }
    }
}

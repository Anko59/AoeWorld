use super::super::Economy;
impl Economy {
    pub(crate) fn hash_vassal_manager(&self, h: &mut blake3::Hasher) {
        let m = &self.vassals;
        h.update(&m.cursor.map_or(u32::MAX, |id| id.0).to_le_bytes());
        h.update(&(m.active.len() as u64).to_le_bytes());
        for id in &m.active {
            h.update(&id.0.to_le_bytes());
        }
        h.update(&(m.controlled.len() as u64).to_le_bytes());
        for (worker, (vassal, resource)) in &m.controlled {
            h.update(&worker.0.to_le_bytes());
            h.update(&vassal.0.to_le_bytes());
            h.update(&[*resource as u8]);
        }
        h.update(&(m.refusal_owners.len() as u64).to_le_bytes());
        for (worker, owner) in &m.refusal_owners {
            h.update(&worker.0.to_le_bytes());
            h.update(&owner.0.to_le_bytes());
        }
        h.update(&(m.manual.len() as u64).to_le_bytes());
        for id in &m.manual {
            h.update(&id.0.to_le_bytes());
        }
        for home in self.vassal_homes.values() {
            home.resources.update_hash(h);
            h.update(&(home.scan_chunk as u64).to_le_bytes());
            h.update(&(home.scan_offset as u64).to_le_bytes());
            h.update(&(home.site_after as u64).to_le_bytes());
            h.update(&[u8::from(home.farm_after.is_some())]);
            if let Some(site) = home.farm_after {
                h.update(&(site as u64).to_le_bytes());
            }
            h.update(&[u8::from(home.repair_after.is_some())]);
            if let Some(farm) = home.repair_after {
                h.update(&farm.0.to_le_bytes());
            }
            h.update(&(home.repair_sites.len() as u64).to_le_bytes());
            for (farm, cursor) in &home.repair_sites {
                h.update(&farm.0.to_le_bytes());
                h.update(&(*cursor as u64).to_le_bytes());
            }
            h.update(&home.worker_after.map_or(u32::MAX, |id| id.0).to_le_bytes());
            h.update(&(home.workers.len() as u64).to_le_bytes());
            for id in &home.workers {
                h.update(&id.0.to_le_bytes());
            }
            for (id, stall) in &home.stalls {
                h.update(&id.0.to_le_bytes());
                h.update(&stall.at.x.to_le_bytes());
                h.update(&stall.at.y.to_le_bytes());
                h.update(&stall.since.to_le_bytes());
                h.update(&[stall.strikes]);
                h.update(
                    &stall
                        .avoid_tile
                        .map_or(i64::MIN, |t| {
                            i64::from(t.x) << 32 | i64::from(t.y) & 0xffff_ffff
                        })
                        .to_le_bytes(),
                );
                h.update(&[stall.avoid_kind.map_or(u8::MAX, |k| k as u8)]);
            }
        }
        // `automatic` is a transient advance guard. Work counters are read-only
        // telemetry; neither affects admission or decisions on subsequent ticks.
    }
}

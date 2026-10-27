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
        for home in self.vassal_homes.values() {
            h.update(&(home.scan_chunk as u64).to_le_bytes());
            h.update(&(home.scan_offset as u64).to_le_bytes());
            h.update(&(home.site_after as u64).to_le_bytes());
            h.update(&home.worker_after.map_or(u32::MAX, |id| id.0).to_le_bytes());
            h.update(&(home.workers.len() as u64).to_le_bytes());
            for id in &home.workers {
                h.update(&id.0.to_le_bytes());
            }
        }
        // `automatic` is a transient advance guard. Work counters are read-only
        // telemetry; neither affects admission or decisions on subsequent ticks.
    }
}

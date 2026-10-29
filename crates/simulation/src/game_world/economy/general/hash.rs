use super::super::Economy;
impl Economy {
    pub(crate) fn hash_generals(&self, h: &mut blake3::Hasher) {
        let m = &self.generals;
        if m.armies.is_empty() && m.recruits.is_empty() {
            return;
        }
        h.update(b"general");
        h.update(&m.cursor.map_or(u32::MAX, |id| id.0).to_le_bytes());
        h.update(&(m.armies.len() as u64).to_le_bytes());
        for (id, a) in &m.armies {
            h.update(&id.0.to_le_bytes());
            h.update(&a.castle.map_or(u32::MAX, |id| id.0).to_le_bytes());
            h.update(&a.home.x.to_le_bytes());
            h.update(&a.home.y.to_le_bytes());
            h.update(&[a.state as u8]);
            h.update(&a.retreat_until.to_le_bytes());
            for n in [a.scan.0, a.scan.1, a.micro_after] {
                h.update(&(n as u64).to_le_bytes());
            }
            for p in [a.destination, a.routed] {
                h.update(&[u8::from(p.is_some())]);
                if let Some(p) = p {
                    h.update(&p.x.to_le_bytes());
                    h.update(&p.y.to_le_bytes());
                }
            }
            h.update(&(a.members.len() as u64).to_le_bytes());
            for id in &a.members {
                h.update(&id.0.to_le_bytes());
            }
            for saved in a.stances {
                h.update(&[u8::from(saved.is_some())]);
                if let Some(saved) = saved {
                    h.update(&saved.id.0.to_le_bytes());
                    h.update(&[saved.original as u8, saved.reason as u8]);
                }
            }
        }
        h.update(&(m.recruits.len() as u64).to_le_bytes());
        for (building, (g, k)) in &m.recruits {
            h.update(&building.0.to_le_bytes());
            h.update(&g.0.to_le_bytes());
            h.update(&[*k as u8]);
        }
        // The controller map is derived from member sets. Work is telemetry.
    }
}

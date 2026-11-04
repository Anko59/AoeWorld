use super::super::Economy;
impl Economy {
    pub(crate) fn hash_vassals(&self, h: &mut blake3::Hasher) {
        let m = &self.vassals;
        if m.domains.is_empty()
            && m.recalls.is_empty()
            && m.returns.is_empty()
            && m.manual.is_empty()
        {
            return;
        }
        h.update(b"vassal-defense");
        h.update(&m.cursor.map_or(u32::MAX, |id| id.0).to_le_bytes());
        h.update(&(m.manual.len() as u64).to_le_bytes());
        for id in &m.manual {
            h.update(&id.0.to_le_bytes());
        }
        h.update(&(m.domains.len() as u64).to_le_bytes());
        for (id, d) in &m.domains {
            for n in [id.0, d.castle.0, u32::from(d.player.0)] {
                h.update(&n.to_le_bytes());
            }
            for n in [d.center.x, d.center.y] {
                h.update(&n.to_le_bytes());
            }
            h.update(&[u8::from(d.development), u8::from(d.status.developed)]);
            h.update(&(d.cursor as u64).to_le_bytes());
            h.update(&[u8::from(d.progress.is_some())]);
            if let Some((site, work, since)) = d.progress {
                h.update(&site.0.to_le_bytes());
                h.update(&work.to_le_bytes());
                h.update(&since.to_le_bytes());
            }
            h.update(&(d.refused.len() as u64).to_le_bytes());
            for (tile, until) in &d.refused {
                h.update(&tile.x.to_le_bytes());
                h.update(&tile.y.to_le_bytes());
                h.update(&until.to_le_bytes());
            }
            for members in [&d.workers, &d.soldiers] {
                h.update(&(members.len() as u64).to_le_bytes());
                for unit in members {
                    h.update(&unit.0.to_le_bytes());
                }
            }
        }
        h.update(&(m.returns.len() as u64).to_le_bytes());
        for (g, goal) in &m.returns {
            h.update(&g.0.to_le_bytes());
            h.update(&goal.x.to_le_bytes());
            h.update(&goal.y.to_le_bytes());
        }
        h.update(&(m.recalls.len() as u64).to_le_bytes());
        for (g, r) in &m.recalls {
            h.update(&g.0.to_le_bytes());
            h.update(&r.vassal.0.to_le_bytes());
            h.update(&[u8::from(r.solo)]);
            for p in [r.destination, Some(r.position), r.movement, Some(r.called)] {
                h.update(&[u8::from(p.is_some())]);
                if let Some(p) = p {
                    h.update(&p.x.to_le_bytes());
                    h.update(&p.y.to_le_bytes());
                }
            }
        }
    }
}

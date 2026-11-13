use super::super::Economy;
impl Economy {
    pub(crate) fn hash_generals(&self, h: &mut blake3::Hasher) {
        let m = &self.generals;
        if !m.appearance_changes.is_empty() {
            h.update(b"sergeant-changes");
            h.update(&(m.appearance_changes.len() as u64).to_le_bytes());
            for id in &m.appearance_changes {
                h.update(&id.0.to_le_bytes());
            }
        }
        if !m.manual.is_empty() {
            h.update(b"general-manual");
            h.update(&(m.manual.len() as u64).to_le_bytes());
            for (id, ordered) in &m.manual {
                h.update(&id.0.to_le_bytes());
                h.update(&ordered.to_le_bytes());
            }
        }
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
            h.update(&[a.state as u8, u8::from(a.solo), u8::from(a.training)]);
            h.update(&a.mix);
            h.update(&a.retreat_until.to_le_bytes());
            for n in [a.scan.0, a.scan.1, a.micro_after] {
                h.update(&(n as u64).to_le_bytes());
            }
            for n in a.production_scan {
                h.update(&(n as u64).to_le_bytes());
            }
            for p in [a.destination, a.routed] {
                h.update(&[u8::from(p.is_some())]);
                if let Some(p) = p {
                    h.update(&p.x.to_le_bytes());
                    h.update(&p.y.to_le_bytes());
                }
            }
            h.update(&[u8::from(a.layout.is_some())]);
            if let Some((goal, heading)) = a.layout {
                for point in [goal, heading] {
                    h.update(&point.x.to_le_bytes());
                    h.update(&point.y.to_le_bytes());
                }
            }
            h.update(&[u8::from(a.commander_march.is_some())]);
            if let Some((to, leg)) = a.commander_march {
                for p in [to, leg] {
                    h.update(&p.x.to_le_bytes());
                    h.update(&p.y.to_le_bytes());
                }
            }
            h.update(&(a.squads.len() as u64).to_le_bytes());
            for squad in &a.squads {
                h.update(&[squad.arm as u8, u8::from(squad.final_leg)]);
                h.update(&(squad.members.len() as u64).to_le_bytes());
                h.update(&[u8::from(squad.march.is_some())]);
                if let Some((to, leg)) = squad.march {
                    h.update(&to.x.to_le_bytes());
                    h.update(&to.y.to_le_bytes());
                    h.update(&leg.x.to_le_bytes());
                    h.update(&leg.y.to_le_bytes());
                }
                for member in &squad.members {
                    h.update(&member.0.to_le_bytes());
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

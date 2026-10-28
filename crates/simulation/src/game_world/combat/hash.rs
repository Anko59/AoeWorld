use super::{Combat, HitRecord};

pub(crate) fn hash_hit(hash: &mut blake3::Hasher, hit: Option<HitRecord>) {
    match hit {
        Some(hit) => {
            hash.update(&[1]);
            hash.update(&hit.attacker.0.to_le_bytes());
            hash.update(&hit.tick.0.to_le_bytes());
        }
        None => {
            hash.update(&[0]);
        }
    }
}

impl Combat {
    pub(crate) fn update_hash(&self, hash: &mut blake3::Hasher) {
        hash.update(&self.search_tick.to_le_bytes());
        hash.update(&self.searches_used.to_le_bytes());
        hash.update(&[u8::from(self.pursuit_cursor.is_some())]);
        if let Some(id) = self.pursuit_cursor {
            hash.update(&id.0.to_le_bytes());
        }
        hash.update(&(self.target_rounds.len() as u64).to_le_bytes());
        for (id, round) in &self.target_rounds {
            hash.update(&id.0.to_le_bytes());
            hash.update(&round.to_le_bytes());
        }
        hash.update(&self.seeker_cursor.0.to_le_bytes());
        hash.update(&self.tower_cursor.0.to_le_bytes());
        hash.update(&(self.seekers.len() as u64).to_le_bytes());
        for id in &self.seekers {
            hash.update(&id.0.to_le_bytes());
        }
        hash.update(&(self.tower_targets.len() as u64).to_le_bytes());
        for (id, target) in &self.tower_targets {
            hash.update(&id.0.to_le_bytes());
            hash.update(&target.0.to_le_bytes());
        }
        hash.update(&self.position_searches.to_le_bytes());
        hash.update(&(self.healers.len() as u64).to_le_bytes());
        for id in &self.healers {
            hash.update(&id.0.to_le_bytes());
        }
        hash.update(&self.next_projectile.to_le_bytes());
        hash.update(&(self.projectiles.len() as u64).to_le_bytes());
        for p in self.projectiles.values() {
            for v in [p.id, p.launched.0, p.impact.0] {
                hash.update(&v.to_le_bytes());
            }
            hash.update(&p.player.0.to_le_bytes());
            for v in [p.attacker.0, p.target.0] {
                hash.update(&v.to_le_bytes());
            }
            for v in [p.from.x, p.from.y, p.to.x, p.to.y] {
                hash.update(&v.to_le_bytes());
            }
            hash.update(&[p.kind as u8]);
            hash.update(&p.attack.to_le_bytes());
            hash.update(&p.bonus.to_le_bytes());
        }
        hash.update(&(self.tower_ready.len() as u64).to_le_bytes());
        for (id, ready) in &self.tower_ready {
            hash.update(&id.0.to_le_bytes());
            hash.update(&ready.to_le_bytes());
        }
        hash.update(&(self.attackers.len() as u64).to_le_bytes());
        for (id, attacker) in &self.attackers {
            hash.update(&id.0.to_le_bytes());
            hash.update(&attacker.target.0.to_le_bytes());
            hash.update(&attacker.repath_tick.to_le_bytes());
            if let Some(origin) = attacker.origin {
                hash.update(&[1]);
                hash.update(&origin.x.to_le_bytes());
                hash.update(&origin.y.to_le_bytes());
            } else {
                hash.update(&[0]);
            }
            if let Some(slot) = attacker.resume {
                hash.update(&[1]);
                hash.update(&slot.x.to_le_bytes());
                hash.update(&slot.y.to_le_bytes());
            } else {
                hash.update(&[0]);
            }
            if let Some(tile) = attacker.goal_tile {
                hash.update(&[1]);
                hash.update(&tile.x.to_le_bytes());
                hash.update(&tile.y.to_le_bytes());
            } else {
                hash.update(&[0]);
            }
            match attacker.far {
                Some(far) => {
                    hash.update(&[1]);
                    for value in [
                        far.centre.x,
                        far.centre.y,
                        far.destination.x,
                        far.destination.y,
                    ] {
                        hash.update(&value.to_le_bytes());
                    }
                }
                None => {
                    hash.update(&[0]);
                }
            }
        }
        hash.update(&(self.resuming.len() as u64).to_le_bytes());
        for (id, resume) in &self.resuming {
            hash.update(&id.0.to_le_bytes());
            hash.update(&resume.slot.x.to_le_bytes());
            hash.update(&resume.slot.y.to_le_bytes());
            hash.update(&[resume.empty]);
        }
        hash.update(&(self.dying.len() as u64).to_le_bytes());
        for (due, id) in &self.dying {
            hash.update(&due.to_le_bytes());
            hash.update(&id.0.to_le_bytes());
        }
    }
}

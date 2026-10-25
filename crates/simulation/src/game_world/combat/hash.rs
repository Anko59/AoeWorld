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
        }
        hash.update(&(self.dying.len() as u64).to_le_bytes());
        for (due, id) in &self.dying {
            hash.update(&due.to_le_bytes());
            hash.update(&id.0.to_le_bytes());
        }
    }
}

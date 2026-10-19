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

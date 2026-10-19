use super::Wildlife;

impl Wildlife {
    pub(crate) fn update_hash(&self, hash: &mut blake3::Hasher) {
        hash.update(&(self.animals.len() as u64).to_le_bytes());
        for a in self.animals.values() {
            let s = a.state;
            hash.update(&s.id.0.to_le_bytes());
            hash.update(&[s.kind as u8, s.task as u8]);
            hash.update(&s.owner.map_or(u32::MAX, |p| u32::from(p.0)).to_le_bytes());
            hash.update(
                &s.target
                    .map_or(u64::MAX, |id| u64::from(id.0))
                    .to_le_bytes(),
            );
            for n in [s.hp, s.food_remaining] {
                hash.update(&n.to_le_bytes());
            }
            for p in [s.position, a.home] {
                hash.update(&p.x.to_le_bytes());
                hash.update(&p.y.to_le_bytes());
            }
            hash.update(&a.cooldown.to_le_bytes());
            hash.update(&a.speed_carry.to_le_bytes());
            hash.update(&a.decay_carry.to_le_bytes());
        }
        hash.update(&(self.hunts.len() as u64).to_le_bytes());
        for (id, job) in &self.hunts {
            hash.update(&id.0.to_le_bytes());
            hash.update(&job.animal.0.to_le_bytes());
            hash.update(&job.cooldown.to_le_bytes());
            hash.update(&job.repath_tick.to_le_bytes());
        }
        hash.update(&(self.due.len() as u64).to_le_bytes());
        for (due, id) in &self.due {
            hash.update(&due.to_le_bytes());
            hash.update(&id.0.to_le_bytes());
        }
    }
}

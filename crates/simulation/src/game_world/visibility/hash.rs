use super::Vision;

impl Vision {
    pub(crate) fn update_hash(&self, hash: &mut blake3::Hasher) {
        hash.update(&[self.disabled as u8]);
        hash.update(&(self.players.len() as u64).to_le_bytes());
        for (id, player) in &self.players {
            hash.update(&id.0.to_le_bytes());
            hash.update(&(player.explored.len() as u64).to_le_bytes());
            for ((x, y), bits) in &player.explored {
                hash.update(&x.to_le_bytes());
                hash.update(&y.to_le_bytes());
                for word in bits {
                    hash.update(&word.to_le_bytes());
                }
            }
            hash.update(&(player.remembered.len() as u64).to_le_bytes());
            for b in player.remembered.values() {
                super::super::economy::training::rally::hash_rally(hash, b.rally_point);
                hash.update(&b.id.0.to_le_bytes());
                hash.update(&b.player.0.to_le_bytes());
                hash.update(&[
                    b.kind as u8,
                    b.state as u8,
                    b.los,
                    b.age as u8,
                    u8::from(b.gate_open),
                ]);
                for c in [
                    b.footprint.min.x,
                    b.footprint.min.y,
                    b.footprint.max.x,
                    b.footprint.max.y,
                ] {
                    hash.update(&c.to_le_bytes());
                }
                for v in [b.hp, b.max_hp] {
                    hash.update(&v.to_le_bytes());
                }
                for v in [b.work_done, b.work_total] {
                    hash.update(&v.to_le_bytes());
                }
                hash.update(&b.food_remaining.map_or(u32::MAX, u32::from).to_le_bytes());
                crate::game_world::combat::hash_hit(hash, b.last_hit);
                hash.update(&[b.research.map_or(255, |r| r.tech as u8)]);
                if let Some(r) = b.research {
                    hash.update(&r.progress_ticks.to_le_bytes());
                    hash.update(&r.total_ticks.to_le_bytes());
                }
                hash.update(&(b.training_queue.len() as u64).to_le_bytes());
                for e in &b.training_queue {
                    hash.update(&[e.kind as u8]);
                    hash.update(&e.progress_ticks.to_le_bytes());
                    hash.update(&e.total_ticks.to_le_bytes());
                }
            }
            hash.update(&(player.resources.len() as u64).to_le_bytes());
            for (id, remaining) in &player.resources {
                hash.update(&id.to_le_bytes());
                hash.update(&remaining.to_le_bytes());
            }
        }
        // Current visibility is derived from live observers. Exploration and
        // remembered state depend on history and therefore belong in replays.
    }
}

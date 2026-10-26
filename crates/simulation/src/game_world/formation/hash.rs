use super::*;

impl GameWorld {
    pub(crate) fn hash_formations(&self, hash: &mut blake3::Hasher) {
        if self.formations.is_empty() {
            return;
        }
        hash.update(b"formations\0");
        hash.update(&(self.formations.len() as u64).to_le_bytes());
        let point = |hash: &mut blake3::Hasher, p: WorldPosition| {
            hash.update(&p.x.to_le_bytes());
            hash.update(&p.y.to_le_bytes());
        };
        for (id, g) in &self.formations {
            hash.update(&id.0.to_le_bytes());
            hash.update(&[
                g.form as u8,
                g.facing as u8,
                u8::from(g.ready),
                u8::from(g.settled),
                u8::from(g.column),
                u8::from(g.attack_move),
            ]);
            for p in [g.anchor, g.heading, g.travel_heading, g.destination] {
                point(hash, p);
            }
            hash.update(&g.speed.0.to_le_bytes());
            hash.update(&g.speed.1.to_le_bytes());
            hash.update(&(g.queue.len() as u64).to_le_bytes());
            for &(p, attack_move) in &g.queue {
                point(hash, p);
                hash.update(&[u8::from(attack_move)]);
            }
            hash.update(&(g.trail.len() as u64).to_le_bytes());
            for &p in &g.trail {
                point(hash, p);
            }
            hash.update(&(g.members.len() as u64).to_le_bytes());
            for m in &g.members {
                hash.update(&m.id.0.to_le_bytes());
                point(hash, m.offset);
                hash.update(&m.carry.to_le_bytes());
                hash.update(&(m.probe as u64).to_le_bytes());
                hash.update(&(m.detour.len() as u64).to_le_bytes());
                for tile in &m.detour {
                    hash.update(&tile.x.to_le_bytes());
                    hash.update(&tile.y.to_le_bytes());
                }
                if let Some((a, b, h, goal)) = m.cache {
                    hash.update(&[1]);
                    for t in [a, b] {
                        hash.update(&t.x.to_le_bytes());
                        hash.update(&t.y.to_le_bytes());
                    }
                    point(hash, h);
                    match goal {
                        steering::Goal::Slot => {
                            hash.update(&[0]);
                        }
                        steering::Goal::Squeeze => {
                            hash.update(&[1]);
                        }
                        steering::Goal::Trail(p) => {
                            hash.update(&[2]);
                            point(hash, p);
                        }
                        steering::Goal::Hold => {
                            hash.update(&[3]);
                        }
                    }
                } else {
                    hash.update(&[0]);
                }
            }
        }
    }
}

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
        for (id, group) in &self.formations {
            hash.update(&id.0.to_le_bytes());
            hash.update(&[
                group.form as u8,
                group.facing as u8,
                u8::from(group.settled),
                u8::from(group.direct),
                u8::from(group.attack_move),
            ]);
            for p in [
                group.anchor,
                group.heading,
                group.travel_heading,
                group.destination,
            ] {
                point(hash, p);
            }
            for value in [
                group.speed.0,
                group.speed.1,
                group.pace.0,
                group.pace.1,
                group.path_cursor as u64,
                group.packing_files as u64,
            ] {
                hash.update(&value.to_le_bytes());
            }
            hash.update(&group.progress.to_le_bytes());
            hash.update(&group.gap.to_le_bytes());
            hash.update(&group.narrow_gap.to_le_bytes());
            hash.update(&group.form_start.to_le_bytes());
            hash.update(&group.form_distance.to_le_bytes());
            hash.update(&group.half_width.to_le_bytes());
            hash.update(&(group.queue.len() as u64).to_le_bytes());
            for &(p, attack_move) in &group.queue {
                point(hash, p);
                hash.update(&[u8::from(attack_move)]);
            }
            hash.update(&(group.path.points.len() as u64).to_le_bytes());
            for (&p, &length) in group.path.points.iter().zip(&group.path.lengths) {
                point(hash, p);
                hash.update(&length.to_le_bytes());
            }
            hash.update(&(group.members.len() as u64).to_le_bytes());
            for member in &group.members {
                hash.update(&member.id.0.to_le_bytes());
                point(hash, member.offset);
                point(hash, member.start_offset);
                point(hash, member.goal);
                point(hash, member.ideal);
                hash.update(&member.carry.to_le_bytes());
                hash.update(&(member.rank as u64).to_le_bytes());
                hash.update(&(member.cursor as u64).to_le_bytes());
                hash.update(&member.progress.to_le_bytes());
                hash.update(&member.stalled.to_le_bytes());
                hash.update(&member.remaining.to_le_bytes());
                hash.update(&member.blocked.to_le_bytes());
                hash.update(&member.detour_progress.to_le_bytes());
                hash.update(&[
                    u8::from(member.arrived),
                    u8::from(member.detached),
                    u8::from(member.following_corridor),
                ]);
            }
        }
    }
}

use super::Economy;

impl Economy {
    pub(crate) fn update_hash(&self, hash: &mut blake3::Hasher) {
        self.hash_garrisons(hash);
        // Pending rendezvous affect future movement and must participate in replay.
        if !self.transport_journeys.is_empty() {
            hash.update(b"shore-rendezvous");
            hash.update(&(self.transport_journeys.len() as u64).to_le_bytes());
        }
        for (ship, job) in &self.transport_journeys {
            hash.update(&ship.0.to_le_bytes());
            hash.update(&job.player.0.to_le_bytes());
            hash.update(&[u8::from(job.unloading), job.passengers.len() as u8]);
            for id in &job.passengers {
                hash.update(&id.0.to_le_bytes());
            }
            for coord in [job.land.x, job.land.y, job.water.x, job.water.y] {
                hash.update(&coord.to_le_bytes());
            }
        }
        self.hash_vassal_manager(hash);
        self.hash_generals(hash);
        hash.update(&(self.vassal_homes.len() as u64).to_le_bytes());
        for (id, home) in &self.vassal_homes {
            hash.update(&id.0.to_le_bytes());
            hash.update(&home.castle.0.to_le_bytes());
            hash.update(&home.center.x.to_le_bytes());
            hash.update(&home.center.y.to_le_bytes());
        }
        hash.update(&(self.cargo.len() as u64).to_le_bytes());
        for (ship, ids) in &self.cargo {
            hash.update(&ship.0.to_le_bytes());
            hash.update(&(ids.len() as u64).to_le_bytes());
            for id in ids {
                hash.update(&id.0.to_le_bytes());
            }
        }
        hash.update(&(self.embarked.len() as u64).to_le_bytes());
        for (id, ship) in &self.embarked {
            hash.update(&id.0.to_le_bytes());
            hash.update(&ship.0.to_le_bytes());
        }
        hash.update(&self.scout_cursor.map_or(u32::MAX, |id| id.0).to_le_bytes());
        hash.update(&(self.scout_searches.len() as u64).to_le_bytes());
        for id in &self.scout_searches {
            hash.update(&id.0.to_le_bytes());
        }
        hash.update(&(self.scouts.len() as u64).to_le_bytes());
        for (id, job) in &self.scouts {
            hash.update(&id.0.to_le_bytes());
            job.update_hash(hash);
        }
        hash.update(&(self.players.len() as u64).to_le_bytes());
        for player in self.players.values() {
            hash.update(&player.player.0.to_le_bytes());
            for amount in [
                player.stockpile.food,
                player.stockpile.wood,
                player.stockpile.gold,
                player.stockpile.stone,
            ] {
                hash.update(&amount.to_le_bytes());
            }
            hash.update(&player.population_used.to_le_bytes());
            hash.update(&[player.age as u8]);
            hash.update(&player.researched.to_le_bytes());
            for price in [
                player.market_prices.wood,
                player.market_prices.food,
                player.market_prices.stone,
            ] {
                hash.update(&price.to_le_bytes());
            }
        }
        hash.update(&(self.open_gates.len() as u64).to_le_bytes());
        for id in &self.open_gates {
            hash.update(&id.0.to_le_bytes());
        }
        hash.update(&(self.buildings.len() as u64).to_le_bytes());
        for building in self.buildings.values() {
            hash.update(&[building.farm_reseeds]);
            if let Some(span) = building.bridge_span {
                for c in [span.from.x, span.from.y, span.to.x, span.to.y] {
                    hash.update(&c.to_le_bytes());
                }
            }
            super::training::rally::hash_rally(hash, building.rally_point);
            hash.update(&building.id.0.to_le_bytes());
            hash.update(&building.player.0.to_le_bytes());
            hash.update(&[building.kind as u8, building.age as u8]);
            if building.kind == super::BuildingKind::Farm {
                hash.update(&[u8::from(self.farm_production.contains_key(&building.id))]);
                if let Some(production) = self.farm_production.get(&building.id) {
                    production.update_hash(hash);
                }
            }
            for coord in [
                building.footprint.min.x,
                building.footprint.min.y,
                building.footprint.max.x,
                building.footprint.max.y,
            ] {
                hash.update(&coord.to_le_bytes());
            }
            crate::game_world::combat::hash_hit(hash, building.last_hit);
            hash.update(&[building.state as u8, building.los]);
            for value in [building.work_done, building.work_total] {
                hash.update(&value.to_le_bytes());
            }
            for value in [building.hp, building.max_hp] {
                hash.update(&value.to_le_bytes());
            }
            hash.update(
                &building
                    .food_remaining
                    .map_or(u32::MAX, u32::from)
                    .to_le_bytes(),
            );
            match building.research {
                None => hash.update(&[0]),
                Some(entry) => {
                    hash.update(&[1, entry.tech as u8]);
                    hash.update(&entry.progress_ticks.to_le_bytes());
                    hash.update(&entry.total_ticks.to_le_bytes())
                }
            };
            hash.update(&(building.training_queue.len() as u64).to_le_bytes());
            for entry in &building.training_queue {
                hash.update(&[entry.kind as u8]);
                hash.update(&entry.progress_ticks.to_le_bytes());
                hash.update(&entry.total_ticks.to_le_bytes());
            }
        }
        hash.update(&(self.gatherers.len() as u64).to_le_bytes());
        for (id, task) in &self.gatherers {
            hash.update(&id.0.to_le_bytes());
            hash.update(&task.node.to_le_bytes());
            hash.update(&[task.fish.map_or(u8::MAX, |fish| fish as u8)]);
            hash.update(&task.farm.map_or(u32::MAX, |farm| farm.0).to_le_bytes());
            hash.update(
                &task
                    .animal
                    .map_or(u64::MAX, |animal| u64::from(animal.0))
                    .to_le_bytes(),
            );
            if task.animal.is_some() {
                hash.update(&[task.carcass_kind.map_or(u8::MAX, |kind| kind as u8)]);
            }
            hash.update(&task.tile.x.to_le_bytes());
            hash.update(&task.tile.y.to_le_bytes());
            hash.update(&[
                task.kind as u8,
                task.phase as u8,
                task.approach_failures,
                u8::from(task.crowded),
            ]);
            hash.update(&task.credits.to_le_bytes());
            match task.dropoff {
                Some(id) => {
                    hash.update(&[1]);
                    hash.update(&id.0.to_le_bytes());
                }
                None => {
                    hash.update(&[0]);
                }
            }
            match task.return_target {
                Some(tile) => {
                    hash.update(&[1]);
                    hash.update(&tile.x.to_le_bytes());
                    hash.update(&tile.y.to_le_bytes());
                }
                None => {
                    hash.update(&[0]);
                }
            }
            match task.work_position {
                Some(at) => {
                    hash.update(&[1]);
                    hash.update(&at.x.to_le_bytes());
                    hash.update(&at.y.to_le_bytes());
                }
                None => {
                    hash.update(&[0]);
                }
            }
            match task.work_tile {
                Some(tile) => {
                    hash.update(&[1]);
                    hash.update(&tile.x.to_le_bytes());
                    hash.update(&tile.y.to_le_bytes());
                }
                None => {
                    hash.update(&[0]);
                }
            }
            hash.update(&(task.failed_dropoffs.len() as u64).to_le_bytes());
            for tile in &task.failed_dropoffs {
                hash.update(&tile.x.to_le_bytes());
                hash.update(&tile.y.to_le_bytes());
            }
            hash.update(&task.retry_tick.to_le_bytes());
        }
        hash.update(&(self.builders.len() as u64).to_le_bytes());
        for (id, task) in &self.builders {
            hash.update(&id.0.to_le_bytes());
            hash.update(&task.building.0.to_le_bytes());
            hash.update(&[task.attempts]);
            hash.update(&(task.queue.len() as u64).to_le_bytes());
            for target in &task.queue {
                hash.update(&target.0.to_le_bytes());
            }
        }
        // Footprints are already hashed as buildings; depletion notifications
        // are a one-tick adapter outbox and do not affect authoritative rules.
    }
}

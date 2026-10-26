//! Movement order validation and issuance, shared by player and task orders.
use super::*;

impl GameWorld {
    pub fn issue_move(
        &mut self,
        id: EntityId,
        destination: WorldPosition,
    ) -> Result<bool, GameWorldError> {
        let destination = self.spread_destination(id, destination)?;
        let result = self.issue_task_move(id, destination)?;
        self.cancel_auto_scout(id);
        self.stop_gathering(id);
        self.cancel_attack(id);
        self.economy.builders.remove(&id);
        if let Some(&index) = self.lookup.get(&id) {
            self.units[index].state.task = if result {
                UnitTask::Moving
            } else if self.units[index].state.carried.is_some() {
                UnitTask::Carrying
            } else {
                UnitTask::Idle
            };
        }
        Ok(result)
    }
}

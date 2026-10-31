use crate::GameplayService;
use aoe_protocol::{GameplayServerMessage, ResumeToken};

impl GameplayService {
    pub(crate) fn world_id(&self) -> u64 {
        self.world_id
    }

    /// Ends all sessions bound to this world before a replacement is activated.
    pub async fn retire(&self, replacement_world_id: u64) {
        self.cancel_preparation();
        let mut sessions = self.sessions.lock().await;
        for session in sessions.values() {
            let _ = session.sender.try_send(GameplayServerMessage::WorldReset {
                world_id: replacement_world_id,
            });
        }
        sessions.clear();
    }

    pub async fn disconnect(&self, session_id: u64) {
        self.sessions.lock().await.remove(&session_id);
    }

    /// Slot occupancy for a match host: AI slots holding a live connection,
    /// AI slots in all, and whether any human slot holds one.
    pub(crate) async fn presence(&self) -> (usize, usize, bool) {
        let sessions = self.sessions.lock().await;
        let (mut live_ai, mut ai, mut human) = (0, 0, false);
        for (index, slot) in self.slots.iter().enumerate() {
            let live = sessions.values().any(|session| session.slot == index);
            if slot.human {
                human |= live;
            } else {
                ai += 1;
                live_ai += usize::from(live);
            }
        }
        (live_ai, ai, human)
    }

    /// Whether this token seats a person: any page holding a human slot's
    /// token may administer the server. AI slots never authorize it.
    pub fn is_human_token(&self, token: ResumeToken) -> bool {
        self.slots
            .iter()
            .any(|slot| slot.human && slot.token == token)
    }

    pub(super) async fn send_to(&self, session_id: u64, message: GameplayServerMessage) {
        let sender = self
            .sessions
            .lock()
            .await
            .get(&session_id)
            .map(|session| session.sender.clone());
        if let Some(sender) = sender {
            let _ = sender.try_send(message);
        }
    }
}

#[path = "gameplay_sessions/tests.rs"]
#[cfg(test)]
mod tests;

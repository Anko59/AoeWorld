use crate::GameplayService;
use aoe_protocol::{GameplayRole, GameplayServerMessage};
use std::time::Instant;

impl GameplayService {
    pub(crate) fn world_id(&self) -> u64 {
        self.world_id
    }

    /// Ends all sessions bound to this world before a replacement is activated.
    pub async fn retire(&self, replacement_world_id: u64) {
        let mut ownership = self.ownership.lock().await;
        let mut sessions = self.sessions.lock().await;
        for session in sessions.values() {
            let _ = session.sender.try_send(GameplayServerMessage::WorldReset {
                world_id: replacement_world_id,
            });
        }
        sessions.clear();
        ownership.controller = None;
    }

    pub async fn disconnect(&self, session_id: u64) {
        let mut ownership = self.ownership.lock().await;
        let mut sessions = self.sessions.lock().await;
        let removed = sessions.remove(&session_id);
        if removed.is_some_and(|session| session.role == GameplayRole::Controller)
            && let Some(lease) = ownership
                .controller
                .as_mut()
                .filter(|lease| lease.session_id == session_id)
        {
            lease.disconnected_at = Some(Instant::now());
        }
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

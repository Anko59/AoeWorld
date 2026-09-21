use crate::GameplayService;
use aoe_protocol::{GameplayRole, GameplayServerMessage, ResumeToken};
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

    /// Returns whether a current gameplay controller owns this resume token.
    pub async fn is_controller(&self, token: ResumeToken) -> bool {
        self.ownership
            .lock()
            .await
            .controller
            .is_some_and(|lease| lease.token == token && lease.disconnected_at.is_none())
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

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
        for slot in &mut ownership.slots {
            slot.lease = None;
        }
    }

    pub async fn disconnect(&self, session_id: u64) {
        let mut ownership = self.ownership.lock().await;
        let mut sessions = self.sessions.lock().await;
        let removed = sessions.remove(&session_id);
        if let Some(index) = removed
            .filter(|session| session.role == GameplayRole::Controller)
            .and_then(|session| session.slot)
            && let Some(lease) = ownership.slots[index]
                .lease
                .as_mut()
                .filter(|lease| lease.session_id == session_id)
        {
            lease.disconnected_at = Some(Instant::now());
        }
    }

    /// Returns whether a connected controller of a human slot owns this token.
    /// AI slots never authorize server administration.
    pub async fn is_controller(&self, token: ResumeToken) -> bool {
        self.ownership.lock().await.slots.iter().any(|slot| {
            slot.human
                && slot.token == token
                && slot
                    .lease
                    .is_some_and(|lease| lease.disconnected_at.is_none())
        })
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

#[cfg(test)]
mod tests {
    use super::*;
    use aoe_core::Seed;
    use aoe_protocol::GameplayServerMessage;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn controller_disconnect_retire_and_delivery_are_observable() {
        let service = GameplayService::new(Seed(1));
        assert!(service.world_id() > 0);

        let (controller_tx, mut controller_rx) = mpsc::channel(8);
        let (controller_id, welcome) = service.register(None, controller_tx).await;
        let token = match welcome {
            GameplayServerMessage::Welcome {
                role, resume_token, ..
            } => {
                assert_eq!(role, GameplayRole::Controller);
                resume_token.expect("controller resume token")
            }
            message => panic!("unexpected welcome: {message:?}"),
        };
        assert!(service.is_controller(token).await);

        let (spectator_tx, mut spectator_rx) = mpsc::channel(8);
        let (spectator_id, welcome) = service.register(None, spectator_tx).await;
        assert!(matches!(
            welcome,
            GameplayServerMessage::Welcome {
                role: GameplayRole::Spectator,
                resume_token: None,
                ..
            }
        ));

        service
            .send_to(
                spectator_id,
                GameplayServerMessage::Error {
                    code: 400,
                    message: "test delivery".to_owned(),
                },
            )
            .await;
        assert!(matches!(
            spectator_rx.try_recv().expect("spectator message"),
            GameplayServerMessage::Error { code: 400, .. }
        ));

        service.disconnect(controller_id).await;
        assert!(!service.is_controller(token).await);

        service.retire(99).await;
        assert!(matches!(
            spectator_rx.try_recv().expect("retirement reset"),
            GameplayServerMessage::WorldReset { world_id: 99 }
        ));
        service
            .send_to(
                spectator_id,
                GameplayServerMessage::Error {
                    code: 500,
                    message: "after retire".to_owned(),
                },
            )
            .await;
        assert!(spectator_rx.try_recv().is_err());
        assert!(controller_rx.try_recv().is_err());
    }
}

use super::network::{NetworkClient, NetworkServer};
use super::report::{ResourceLifecycleEvidence, SourceQualificationError};
use crate::{GameplayService, PageResidency};
use aoe_core::TileRect;
use aoe_map::{EnvironmentPageProvider, MapPackage, ResourceNode, ResourceOverlaySnapshot};
use aoe_protocol::{
    GameplayClientMessage, GameplayRole, GameplayServerMessage, ResourceAmount, ResourceState,
    ResumeToken,
};
use std::{path::Path, sync::Arc};

pub(super) struct LifecycleRun {
    pub(super) evidence: ResourceLifecycleEvidence,
    pub(super) persisted_snapshot: ResourceOverlaySnapshot,
    pub(super) restart_provider_resident_pages: usize,
}

pub(super) async fn exercise_resource_lifecycle(
    service: GameplayService,
    package: &MapPackage,
    package_directory: &Path,
    state_directory: &Path,
    node: &ResourceNode,
) -> Result<LifecycleRun, SourceQualificationError> {
    let original_server = NetworkServer::start(service.clone()).await?;
    let (mut initial, welcome) = NetworkSession::open(original_server.address(), None).await?;
    let resume_token = controller_resume_token(&welcome, "initial registration")?;
    let original_network_world_id = welcome_world_id(&welcome)?;
    let initial_state = subscribe_and_tick(&mut initial, &service, 1).await?;
    assert_initial_snapshot(&initial_state)?;

    let depleted = service
        .deplete_resource_persisted(node.id, node.initial_amount)
        .await?;
    if depleted.depletion.remaining != 0 || !depleted.depletion.became_nonblocking {
        return Err(SourceQualificationError::NoResource);
    }
    service.tick().await;
    let delta = next_resource_state(&mut initial, "network depletion delta").await?;
    assert_depletion_delta(&delta, node)?;
    drop(initial);

    let (mut reconnect, welcome) =
        NetworkSession::open(original_server.address(), Some(resume_token)).await?;
    if controller_resume_token(&welcome, "resume registration")? != resume_token
        || welcome_world_id(&welcome)? != original_network_world_id
    {
        return Err(lifecycle_mismatch("network resume welcome"));
    }
    let reconnect_state = subscribe_and_tick(&mut reconnect, &service, 2).await?;
    if reconnect_state.from_revision.is_some()
        || reconnect_state.revision != delta.revision
        || reconnect_state.changes != delta.changes
    {
        return Err(lifecycle_mismatch("network resume snapshot"));
    }
    drop(reconnect);
    original_server.stop().await?;

    let persisted_snapshot = service
        .resource_snapshot()
        .await
        .ok_or(SourceQualificationError::NoResource)?;
    let original_world_id = service.world_id();
    drop(service);

    let restart_provider = PageResidency::open(package_directory, package, &|| false)?;
    let restarted = GameplayService::from_stored_map(
        package.clone(),
        Some(restart_provider.clone() as Arc<dyn EnvironmentPageProvider>),
        Some(state_directory.to_owned()),
        &|| false,
    )?
    .ok_or(SourceQualificationError::NoStart)?;
    if restarted.world_id() == original_world_id {
        return Err(lifecycle_mismatch("changed restart world id"));
    }
    let restarted_snapshot = restarted
        .resource_snapshot()
        .await
        .ok_or(SourceQualificationError::NoResource)?;
    if restarted_snapshot != persisted_snapshot {
        return Err(SourceQualificationError::OverlayMismatch);
    }

    let restarted_server = NetworkServer::start(restarted.clone()).await?;
    let (mut fresh, welcome) = NetworkSession::open(restarted_server.address(), None).await?;
    let restarted_network_world_id = welcome_world_id(&welcome)?;
    if restarted_network_world_id == original_network_world_id {
        return Err(lifecycle_mismatch("network restarted world id"));
    }
    let fresh_state = subscribe_and_tick(&mut fresh, &restarted, 1).await?;
    if fresh_state.from_revision.is_some()
        || fresh_state.revision != persisted_snapshot.revision
        || fresh_state.changes != snapshot_amounts(&persisted_snapshot)
    {
        return Err(lifecycle_mismatch("network post-restart snapshot"));
    }
    drop(fresh);
    restarted_server.stop().await?;

    Ok(LifecycleRun {
        evidence: ResourceLifecycleEvidence {
            initial_snapshot_count: 1,
            resource_mutation_count: 1,
            delta_snapshot_count: 1,
            resume_reconnect_snapshot_count: 1,
            restart_count: 1,
            persisted_snapshot_replay_count: 1,
            post_restart_client_snapshot_count: 1,
            page_residency_recreation_count: 1,
            stored_map_recreation_count: 1,
            network_connection_count: 3,
            resume_token_reconnect_verified: true,
            changed_world_id_verified: true,
            persisted_snapshot_verified: true,
            post_restart_client_snapshot_verified: true,
            network_resume_snapshot_verified: true,
            network_post_restart_snapshot_verified: true,
        },
        persisted_snapshot,
        restart_provider_resident_pages: restart_provider.resident_pages(),
    })
}

struct NetworkSession {
    client: Option<NetworkClient>,
}

impl NetworkSession {
    async fn open(
        address: std::net::SocketAddr,
        resume_token: Option<ResumeToken>,
    ) -> Result<(Self, GameplayServerMessage), SourceQualificationError> {
        let (client, welcome) =
            tokio::task::spawn_blocking(move || NetworkClient::open(address, resume_token))
                .await
                .map_err(|_| lifecycle_mismatch("network open join"))??;
        Ok((
            Self {
                client: Some(client),
            },
            welcome,
        ))
    }

    async fn send(
        &mut self,
        message: GameplayClientMessage,
    ) -> Result<(), SourceQualificationError> {
        let mut client = self
            .client
            .take()
            .ok_or_else(|| lifecycle_mismatch("network send on closed connection"))?;
        let (client, sent) = tokio::task::spawn_blocking(move || {
            let sent = client.send(&message);
            (client, sent)
        })
        .await
        .map_err(|_| lifecycle_mismatch("network send join"))?;
        self.client = Some(client);
        sent
    }

    async fn next(&mut self) -> Result<Option<GameplayServerMessage>, SourceQualificationError> {
        let mut client = self
            .client
            .take()
            .ok_or_else(|| lifecycle_mismatch("network receive on closed connection"))?;
        let (client, received) = tokio::task::spawn_blocking(move || {
            let received = client.next();
            (client, received)
        })
        .await
        .map_err(|_| lifecycle_mismatch("network receive join"))?;
        if received.is_ok() {
            self.client = Some(client);
        }
        received
    }
}

async fn subscribe_and_tick(
    client: &mut NetworkSession,
    service: &GameplayService,
    revision: u64,
) -> Result<ResourceState, SourceQualificationError> {
    client
        .send(GameplayClientMessage::Subscribe {
            revision,
            region: TileRect::from_xywh(0, 0, 64, 64),
        })
        .await?;
    loop {
        match client.next().await? {
            Some(GameplayServerMessage::Snapshot { .. }) => break,
            Some(GameplayServerMessage::Error { .. }) | None => {
                return Err(lifecycle_mismatch("network subscription snapshot"));
            }
            Some(_) => {}
        }
    }
    service.tick().await;
    next_resource_state(client, "network resource snapshot").await
}

async fn next_resource_state(
    client: &mut NetworkSession,
    stage: &'static str,
) -> Result<ResourceState, SourceQualificationError> {
    while let Some(message) = client.next().await? {
        match message {
            GameplayServerMessage::ResourceState(state) => return Ok(state),
            GameplayServerMessage::Error { .. } => return Err(lifecycle_mismatch(stage)),
            _ => {}
        }
    }
    Err(lifecycle_mismatch(stage))
}

fn assert_initial_snapshot(state: &ResourceState) -> Result<(), SourceQualificationError> {
    if state.from_revision.is_some() || state.revision != 0 || !state.changes.is_empty() {
        return Err(lifecycle_mismatch("network initial snapshot"));
    }
    Ok(())
}

fn assert_depletion_delta(
    state: &ResourceState,
    node: &ResourceNode,
) -> Result<(), SourceQualificationError> {
    if state.from_revision != Some(0)
        || state.revision != 1
        || state.changes
            != vec![ResourceAmount {
                id: node.id,
                remaining: 0,
            }]
    {
        return Err(lifecycle_mismatch("network depletion delta"));
    }
    Ok(())
}

fn controller_resume_token(
    welcome: &GameplayServerMessage,
    stage: &'static str,
) -> Result<ResumeToken, SourceQualificationError> {
    let GameplayServerMessage::Welcome {
        role, resume_token, ..
    } = welcome
    else {
        return Err(lifecycle_mismatch(stage));
    };
    if *role != GameplayRole::Controller {
        return Err(lifecycle_mismatch(stage));
    }
    resume_token.ok_or_else(|| lifecycle_mismatch(stage))
}

fn welcome_world_id(welcome: &GameplayServerMessage) -> Result<u64, SourceQualificationError> {
    let GameplayServerMessage::Welcome { world_id, .. } = welcome else {
        return Err(lifecycle_mismatch("network welcome world id"));
    };
    Ok(*world_id)
}

fn snapshot_amounts(snapshot: &ResourceOverlaySnapshot) -> Vec<ResourceAmount> {
    snapshot
        .changes
        .iter()
        .map(|change| ResourceAmount {
            id: change.id,
            remaining: change.remaining,
        })
        .collect()
}

fn lifecycle_mismatch(stage: &'static str) -> SourceQualificationError {
    SourceQualificationError::Lifecycle { stage }
}

#[cfg(test)]
mod tests;

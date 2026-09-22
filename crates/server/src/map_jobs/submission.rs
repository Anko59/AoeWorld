use super::{Job, Manager, StartError, preparation::PreparationPreference};
use aoe_map::MapRequest;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Identity {
    pub key: String,
    pub preference: PreparationPreference,
}
impl Identity {
    pub fn new(
        key: Option<String>,
        preference: PreparationPreference,
    ) -> Result<Option<Self>, String> {
        key.map(|key| {
            let value = Self { key, preference };
            value.validate()?;
            Ok(value)
        })
        .transpose()
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.key.len() != 32
            || !self
                .key
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err("submission key must be 32 lowercase hexadecimal characters".into());
        }
        Ok(())
    }
}

pub(super) fn existing(
    manager: &Manager,
    identity: Option<&Identity>,
    request: MapRequest,
) -> Result<Option<Job>, StartError> {
    let Some(identity) = identity else {
        return Ok(None);
    };
    for entry in manager.jobs.values() {
        if let Some(previous) = &entry.submission
            && previous.key == identity.key
        {
            if entry.job.request != request || previous.preference != identity.preference {
                return Err(StartError::Conflict(
                    "submission key was already used for another request".into(),
                ));
            }
            return Ok(Some(entry.snapshot()));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests;

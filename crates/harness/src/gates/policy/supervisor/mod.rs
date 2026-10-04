//! Supervisor requirements and lease models, never trusted execution or admission.
mod artifact;
mod cli;
mod identity;
mod journal;
mod lease;
mod requirements;
#[cfg(test)]
mod tests;
mod transport;
use super::{
    Anchor, Result,
    descriptor::{Abi, Operation},
    digest, full_oid, plain_absolute,
};
pub(crate) use cli::{Options, execute};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

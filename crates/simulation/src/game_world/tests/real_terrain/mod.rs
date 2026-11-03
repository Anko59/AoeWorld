//! Continuous bridge traversal on typed hydrology, with exact arrival metrics.
use super::far_orders::center;
use super::group_baseline::forest;
use super::*;

mod bridge_march;
mod budget;
mod crowded;
mod general_followers;
mod general_opposed;
mod landscapes;
mod metrics;
mod terrain;
mod unreachable_enemy;
use metrics::{measure, reference_ticks};

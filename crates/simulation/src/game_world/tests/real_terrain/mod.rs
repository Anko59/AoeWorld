//! Continuous bridge traversal on typed hydrology, with exact arrival metrics.
use super::far_orders::center;
use super::group_baseline::forest;
use super::*;

mod armies;
mod bridge_march;
mod budget;
mod crowded;
mod general_followers;
mod general_opposed;
mod landscapes;
mod lane_tests;
mod metrics;
mod recovery_tests;
mod terrain;
mod unreachable_enemy;
mod vassal_river;
use metrics::{measure, reference_ticks};

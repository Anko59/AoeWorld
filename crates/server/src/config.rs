use aoe_scenario::{Scenario, named};
use std::{env, net::SocketAddr};

#[derive(Debug)]
pub struct Config {
    pub bind: SocketAddr,
    pub scenario: Scenario,
    pub tick_hz: u32,
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        let bind = env::var("AOE_BIND")
            .unwrap_or_else(|_| "127.0.0.1:8080".to_owned())
            .parse()
            .map_err(|e| format!("invalid AOE_BIND: {e}"))?;
        let name = env::var("AOE_SCENARIO").unwrap_or_else(|_| "smoke".to_owned());
        let scenario = named(&name).ok_or_else(|| format!("unknown AOE_SCENARIO: {name}"))?;
        let tick_hz: u32 = env::var("AOE_TICK_HZ")
            .unwrap_or_else(|_| "20".to_owned())
            .parse()
            .map_err(|e| format!("invalid AOE_TICK_HZ: {e}"))?;
        if !(1..=60).contains(&tick_hz) {
            return Err("AOE_TICK_HZ must be 1..=60".to_owned());
        }
        Ok(Self {
            bind,
            scenario,
            tick_hz,
        })
    }
}

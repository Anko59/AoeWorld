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
        let bind = env::var("AOE_BIND").unwrap_or_else(|_| "127.0.0.1:8080".to_owned());
        let name = env::var("AOE_SCENARIO").unwrap_or_else(|_| "smoke".to_owned());
        let tick_hz = env::var("AOE_TICK_HZ").unwrap_or_else(|_| "20".to_owned());
        Self::parse(&bind, &name, &tick_hz)
    }

    fn parse(bind: &str, name: &str, tick_hz: &str) -> Result<Self, String> {
        let bind = bind.parse().map_err(|e| format!("invalid AOE_BIND: {e}"))?;
        let scenario = named(name).ok_or_else(|| format!("unknown AOE_SCENARIO: {name}"))?;
        let tick_hz: u32 = tick_hz
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_and_rejects_invalid_configuration() {
        let config = Config::parse("127.0.0.1:0", "target-hotspot", "20").expect("valid config");
        assert_eq!(config.tick_hz, 20);
        assert_eq!(config.scenario.name, "target-hotspot");
        assert!(Config::parse("bad", "smoke", "20").is_err());
        assert!(Config::parse("127.0.0.1:0", "missing", "20").is_err());
        assert!(Config::parse("127.0.0.1:0", "smoke", "bad").is_err());
        assert!(Config::parse("127.0.0.1:0", "smoke", "0").is_err());
        assert!(Config::parse("127.0.0.1:0", "smoke", "61").is_err());
    }
}

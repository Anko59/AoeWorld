//! The bot loop: one decision per decision point, orders sent right after.
use crate::{
    client::{Client, Decision},
    error::Result,
    events::Event,
    orders::Order,
};

/// A player brain. `decide` sees the world as the slot knows it plus the
/// events since the last decision, and pushes the orders it wants sent.
pub trait Bot {
    fn decide(&mut self, decision: &Decision, orders: &mut Vec<Order>);
}

impl<F: FnMut(&Decision, &mut Vec<Order>)> Bot for F {
    fn decide(&mut self, decision: &Decision, orders: &mut Vec<Order>) {
        self(decision, orders);
    }
}

/// When a bot loop stops on its own.
#[derive(Clone, Copy, Debug, Default)]
pub struct RunLimits {
    /// Stop once the game clock reaches this tick.
    pub until_tick: Option<u64>,
    /// Stop after this many decisions.
    pub max_decisions: Option<u64>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RunSummary {
    pub decisions: u64,
    pub orders_sent: u64,
    pub orders_accepted: u64,
    pub orders_rejected: u64,
    pub last_tick: u64,
}

/// Runs `bot` against `client` until a limit is reached or the connection
/// ends. A closed connection returns the summary so far with the error.
pub async fn run_bot(
    client: &Client,
    bot: &mut impl Bot,
    limits: RunLimits,
) -> (RunSummary, Result<()>) {
    let mut summary = RunSummary::default();
    loop {
        let decision = match client.next_decision().await {
            Ok(decision) => decision,
            Err(error) => return (summary, Err(error)),
        };
        summary.decisions += 1;
        summary.last_tick = decision.world.tick();
        for event in &decision.events {
            if let Event::OrderResult { result, .. } = event {
                if *result == crate::model::CommandResult::Accepted {
                    summary.orders_accepted += 1;
                } else {
                    summary.orders_rejected += 1;
                }
            }
        }
        let mut orders = Vec::new();
        bot.decide(&decision, &mut orders);
        for order in orders {
            client.send(order);
            summary.orders_sent += 1;
        }
        if limits
            .until_tick
            .is_some_and(|tick| summary.last_tick >= tick)
            || limits.max_decisions.is_some_and(|n| summary.decisions >= n)
        {
            return (summary, Ok(()));
        }
    }
}

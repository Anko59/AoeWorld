//! Credited deliveries, grouped by simulation time rather than worker jobs.
use super::super::{CarriedResource, Economy};
use aoe_core::PlayerId;

const BUCKETS: usize = 12;
const BUCKET_SECONDS: u64 = 5;

#[derive(Clone, Copy, Debug, Default)]
struct Bucket {
    epoch: Option<u64>,
    amounts: [u64; 4],
}

#[derive(Debug, Default)]
pub(crate) struct Income {
    buckets: [Bucket; BUCKETS],
}

impl Income {
    fn record(&mut self, epoch: u64, kind: usize, amount: u32) {
        let bucket = &mut self.buckets[(epoch % BUCKETS as u64) as usize];
        if bucket.epoch != Some(epoch) {
            *bucket = Bucket {
                epoch: Some(epoch),
                amounts: [0; 4],
            };
        }
        bucket.amounts[kind] = bucket.amounts[kind].saturating_add(u64::from(amount));
    }

    pub(crate) fn recent(&self, tick: u64, tick_hz: u32) -> [u64; 4] {
        let epoch = tick / (BUCKET_SECONDS * u64::from(tick_hz));
        let mut totals = [0_u64; 4];
        for bucket in &self.buckets {
            if bucket
                .epoch
                .is_some_and(|at| at <= epoch && epoch - at < BUCKETS as u64)
            {
                for (total, amount) in totals.iter_mut().zip(bucket.amounts) {
                    *total = total.saturating_add(amount);
                }
            }
        }
        totals
    }

    pub(crate) fn update_hash(&self, hash: &mut blake3::Hasher) {
        for bucket in &self.buckets {
            hash.update(&[u8::from(bucket.epoch.is_some())]);
            if let Some(epoch) = bucket.epoch {
                hash.update(&epoch.to_le_bytes());
            }
            for amount in bucket.amounts {
                hash.update(&amount.to_le_bytes());
            }
        }
    }
}

impl Economy {
    /// Called only at real delivery contact. Refunds and stock edits are not income.
    pub(crate) fn credit_delivery(
        &mut self,
        owner: PlayerId,
        load: CarriedResource,
        tick: u64,
        tick_hz: u32,
    ) -> bool {
        let Some(player) = self.players.get_mut(&owner) else {
            return false;
        };
        let before = match load.kind {
            aoe_map::ResourceKind::Food => player.stockpile.food,
            aoe_map::ResourceKind::Wood => player.stockpile.wood,
            aoe_map::ResourceKind::Gold => player.stockpile.gold,
            aoe_map::ResourceKind::Stone => player.stockpile.stone,
        };
        player.stockpile.deposit(load);
        let credited = u32::from(load.amount).min(u32::MAX - before);
        if credited > 0 {
            self.income.entry(owner).or_default().record(
                tick / (BUCKET_SECONDS * u64::from(tick_hz)),
                load.kind as usize,
                credited,
            );
        }
        true
    }
}

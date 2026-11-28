use super::*;

fn worker_player(world: &mut GameWorld, researched: u128) {
    world.economy.players.insert(
        PlayerId(0),
        crate::PlayerEconomy {
            player: PlayerId(0),
            stockpile: crate::Stockpile::default(),
            population_used: 1,
            upkeep_remainder: 0,
            upkeep_shortage: false,
            upkeep_slow_ticks: 0,
            population_cap: 5,
            age: Age::Feudal,
            researched: researched | Tech::FeudalAge.bit(),
            market_prices: crate::MarketPrices::default(),
        },
    );
}

#[test]
fn gathering_uses_the_exact_researched_rates_on_real_resource_depletion() {
    for (kind, tech, expected) in [
        (ResourceKind::Wood, Tech::DoubleBitAxe, 9),
        (ResourceKind::Gold, Tech::GoldMining, 8),
        (ResourceKind::Stone, Tech::StoneMining, 8),
    ] {
        for upgraded in [false, true] {
            let (mut world, node, worker) = resource_world(kind);
            worker_player(&mut world, if upgraded { tech.bit() } else { 0 });
            let initial = world.resource_remaining(node.id).unwrap();
            world.issue_gather(PlayerId(0), worker, node.id).unwrap();
            advance(&mut world, 400);
            assert_eq!(
                initial - world.resource_remaining(node.id).unwrap(),
                if upgraded { expected } else { 7 }
            );
        }
    }
}

#[test]
fn wheelbarrow_fills_and_returns_thirteen_resource_loads() {
    let (mut world, node, worker) = resource_world(ResourceKind::Wood);
    worker_player(&mut world, Tech::Wheelbarrow.bit());
    world.issue_gather(PlayerId(0), worker, node.id).unwrap();
    advance(&mut world, 1000);
    assert_eq!(world.unit(worker).unwrap().carried.unwrap().amount, 13);
}

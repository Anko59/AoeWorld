"""Exploration frontiers must move on from blocked resource tiles."""
from aoeworld import CommandResult, UnitKind
from aoe_ai import Brain
from fakes import FakeWorld, base, economy, order_result, unit
from test_logic import play


def frontier_world(tick):
    units, buildings = base(12)
    world = FakeWorld(units + [unit(200, UnitKind.SCOUT, tile=(50, 50))], buildings,
                      tick=tick, eco=economy(population=14, cap=30))
    world.land_route = lambda x, y, tx, ty: None if (tx, ty) == (200, 50) else [(x, y), (tx, ty)]
    world.land_frontier = lambda *args: (70, 50)
    return world


def first_frontier():
    brain = Brain()
    brain.board.scout_waypoints = [(200, 50)]
    brain, client = play(frontier_world(100), brain=brain)
    assert [a for a in client.named("move") if a[0] == 200] == [(200, 70, 50)]
    sequence = next(i+1 for i, (name, args) in enumerate(client.calls) if name == "move" and args[0] == 200)
    return brain, sequence


def test_a_refused_frontier_uses_another_proven_land_tile():
    brain, sequence = first_frontier()
    refusal = order_result(sequence, CommandResult.REJECTED_INVALID_DESTINATION)
    _, client = play(frontier_world(120), brain=brain, events=[refusal])
    moves = [a for a in client.named("move") if a[0] == 200]
    assert moves and all(a[1:] != (70, 50) for a in moves)


def test_an_idle_scout_does_not_repeat_the_same_unreached_frontier_forever():
    brain, _ = first_frontier()
    _, client = play(frontier_world(200), brain=brain)
    moves = [a for a in client.named("move") if a[0] == 200]
    assert moves and all(a[1:] != (70, 50) for a in moves)


def test_engaging_soldiers_are_not_recalled_to_exploration_in_the_same_decision():
    units, buildings = base(12)
    army = [unit(300+i, UnitKind.SPEARMAN, tile=(150, 150)) for i in range(14)]
    foe = unit(900, UnitKind.SPEARMAN, tile=(153, 150), mine=False)
    _, client = play(FakeWorld(units + army, buildings, enemies=[foe],
                               eco=economy(population=27, cap=40)))
    assert [a for a in client.named("attack") if a[0] >= 300]
    assert [a for a in client.named("move") if a[0] >= 300] == []


def test_far_rally_uses_short_land_hops_and_keeps_a_pause():
    units, buildings = base(12)
    army = [unit(300, UnitKind.SPEARMAN, tile=(200, 55))]
    world = lambda tick: FakeWorld(units + army, buildings, tick=tick,
                                   eco=economy(population=14, cap=30))
    brain, client = play(world(100))
    moves = [a for a in client.named("move") if a[0] == 300]
    assert len(moves) == 1
    from aoe_ai.geometry import dist
    assert dist((200, 55), moves[0][1:]) <= 48
    _, client = play(world(120), brain=brain)
    assert [a for a in client.named("move") if a[0] == 300] == []
    blocked = world(300)
    blocked.land_route = lambda *args: None
    _, client = play(blocked, brain=brain)
    assert [a for a in client.named("move") if a[0] == 300] == []


def test_economy_chooses_resources_on_the_workers_land_component():
    from aoeworld import ResourceKind
    from aoe_ai.context import Context
    from aoe_ai.config import Config
    from aoe_ai.blackboard import Blackboard
    from fakes import FakeClient, decision, node
    units, buildings = base(1)
    worker = units[0]
    across = node(700, ResourceKind.WOOD, (58, 50))
    reachable = node(701, ResourceKind.WOOD, (50, 64))
    world = FakeWorld(units, buildings, nodes=[across, reachable])
    world.land_route = lambda x, y, tx, ty: None if tx > 55 else [(x, y), (tx, ty)]
    ctx = Context(FakeClient(), decision(world), Blackboard(), Config())
    assert Brain().economy.best_node(ctx, worker, ResourceKind.WOOD).id == 701


def test_town_center_rally_does_not_send_new_workers_across_a_river():
    from aoeworld import ResourceKind
    from fakes import node
    units, buildings = base(1)
    world = FakeWorld(units, buildings, nodes=[node(700, ResourceKind.WOOD, (58, 50)),
                                              node(701, ResourceKind.WOOD, (50, 64))])
    world.land_route = lambda x, y, tx, ty: None if tx > 55 else [(x, y), (tx, ty)]
    _, client = play(world)
    assert client.named("set_rally_resource") == [(500, 701)]


def test_worker_exploration_stays_on_known_connected_land():
    from aoe_ai.context import Context
    from aoe_ai.config import Config
    from aoe_ai.blackboard import Blackboard
    from fakes import FakeClient, decision
    units, buildings = base(1)
    world = FakeWorld(units, buildings)
    world.land_route = lambda x, y, tx, ty: None if tx > 55 else [(x, y), (tx, ty)]
    world.land_frontier = lambda *args: (50, 65)
    client = FakeClient()
    ctx = Context(client, decision(world), Blackboard(), Config())
    Brain().economy.explore(ctx, units[0])
    assert client.named("move") and all(args[1] <= 55 for args in client.named("move"))

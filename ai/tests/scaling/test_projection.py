"""Expeditions use ordinary orders, public terrain and a bounded group."""
from aoeworld import BuildingKind, UnitKind

from aoe_ai.play.constructor import Constructor
from aoe_ai.play.projection import Expedition, ProjectionManager
from fakes import building, unit
from test_kingdom import kingdom


def campaign():
    ctx = kingdom(villagers=40, centers=1)
    ctx.tick = ctx.world.tick = 25 * 60 * ctx.world.tick_hz
    ctx.army = [unit(2000 + i, UnitKind.SPEARMAN, (55, 55)) for i in range(20)]
    ctx.world._units += ctx.army
    ctx.world._enemy_buildings = [building(9000, BuildingKind.BARRACKS, (250, 50), mine=False)]
    ctx.world.water_passable = lambda x, y: 100 <= x < 110
    ctx.world.water_frontier = lambda x, y, tx, ty: (min(109, x + 6), y)
    ctx.world.find_bridge_site = lambda *args: None
    ctx.world.unit = lambda identity: next((u for u in ctx.world._units if u.id == identity), None)
    ctx.world.building = lambda identity: next((b for b in ctx.buildings + ctx.world._enemy_buildings if b.id == identity), None)
    return ctx, ProjectionManager(Constructor())


def test_expedition_stages_a_bounded_army_before_issuing_attack_orders():
    ctx, manager = campaign()
    manager.prepare(ctx)
    manager.step(ctx)
    assert ctx.board.expedition is not None
    assert len(ctx.board.expedition.crew) <= ctx.config.projection_army_size
    assert ctx.client.named('formation_move')
    assert not ctx.client.named('attack')
    assert len(ctx.client.calls) <= 4


def test_staged_army_builds_a_needed_bridge_in_castle_age():
    ctx, manager = campaign()
    manager.prepare(ctx)
    exp = ctx.board.expedition
    exp.phase = 'advance'
    ctx.world.land_route = lambda x,y,tx,ty: [(x,y),(tx,ty)] if tx < 100 else None
    ctx.world.land_frontier = lambda *a: (99, 55)
    ctx.world.find_bridge_site = lambda *a: ((99,55),(110,55))
    manager.step(ctx)
    assert ctx.client.named('build_bridge'), 'a Castle army must not lose the Feudal crossing capability'


def test_unloaded_veterans_establish_forward_production_before_attacking():
    ctx, manager = campaign()
    exp = Expedition(ctx.home, (253, 53), [2000,2001,2002,2003], ctx.villagers[0].id,
                     phase='outpost', target=9000, crossed='water', started=ctx.tick)
    ctx.board.expedition = exp
    for u in ctx.world._units:
        if u.id in exp.crew or u.id == exp.builder:
            u.tile = (220, 55)
    manager.prepare(ctx)
    manager.step(ctx)
    assert any(a[1] in (BuildingKind.BARRACKS, BuildingKind.SIEGE_WORKSHOP) for a in ctx.client.named('build'))
    assert not ctx.client.named('attack')


def test_transport_load_is_small_enough_for_the_five_passenger_capacity():
    ctx, manager = campaign()
    ship = unit(8000, UnitKind.TRANSPORT_SHIP, (101,55))
    ship.cargo_count = 0
    ctx.world._units.append(ship)
    exp = Expedition(ctx.home, (253,53), [u.id for u in ctx.army[:16]], ctx.villagers[0].id,
                     phase='board', transport=ship.id, started=ctx.tick)
    ctx.board.expedition = exp
    manager.prepare(ctx)
    manager.step(ctx)
    loads = ctx.client.named('load')
    assert loads and len(loads[0][0]) <= 5
    assert exp.builder in loads[0][0]


def test_expedition_reserves_its_units_from_home_rallies_and_economic_reassignment():
    ctx, manager = campaign()
    manager.prepare(ctx)
    exp = ctx.board.expedition
    assert set(exp.crew) <= ctx.busy
    assert exp.builder in ctx.busy


def test_forward_training_and_attacks_share_the_existing_order_budget():
    ctx, manager = campaign()
    manager.prepare(ctx)
    ctx.order_limit = 4
    manager.step(ctx)
    assert ctx.orders <= 4 and ctx._land_queries <= ctx.config.land_queries_per_decision


def test_boarded_units_may_disappear_from_the_live_view_without_ending_the_mission():
    ctx, manager = campaign()
    ship = unit(8000, UnitKind.TRANSPORT_SHIP, (101,55))
    ship.cargo_count = 5
    ctx.world._units.append(ship)
    exp = Expedition(ctx.home, (253,53), [3000,3001,3002,3003], 3004,
                     phase='board', transport=ship.id, started=ctx.tick,
                     passengers=[3000,3001,3002,3003,3004])
    ctx.board.expedition = exp
    manager.step(ctx)
    assert ctx.board.expedition is exp and exp.loaded and exp.phase == 'sail'
    assert ctx.board.stats['transports_used'] == 1


def test_water_crossing_requires_observed_passengers_after_an_actual_voyage():
    ctx, manager = campaign()
    ship = unit(8000, UnitKind.TRANSPORT_SHIP, (111,55))
    ctx.world._units.append(ship)
    exp = Expedition(ctx.home, (253,53), [2000,2001], ctx.villagers[0].id,
                     phase='sail', transport=ship.id, started=ctx.tick,
                     passengers=[2000,2001,ctx.villagers[0].id], loaded=True, departure=(101,55))
    ctx.board.expedition = exp
    for u in ctx.world._units:
        if u.id in exp.passengers:
            u.tile = (113,55)
    manager.step(ctx)
    assert exp.crossed == 'water' and exp.phase == 'advance'
    assert exp.proof['departure'] != exp.proof['arrival']
    assert ctx.board.stats['transport_landings'] == 1


def test_finishing_restores_automatic_view_even_during_view_cooldown():
    ctx, manager = campaign()
    manager.prepare(ctx)
    manager.focus(ctx, (55,55))
    manager.finish(ctx)
    assert ctx.client.named('focus_view')[-1] == (None,)


def test_army_at_the_last_reachable_shore_starts_boarding_without_a_frontier_hop():
    ctx, manager = campaign()
    for u in ctx.army:
        u.tile = (99,55)
    manager.prepare(ctx)
    exp = ctx.board.expedition
    exp.phase = 'advance'
    ctx.world.land_route = lambda *a: None
    ctx.world.land_frontier = lambda *a: None
    manager.step(ctx)
    assert exp.phase == 'board'


def test_losing_a_loaded_transport_aborts_instead_of_switching_to_another_ship():
    ctx, manager = campaign()
    lost = unit(8000, UnitKind.TRANSPORT_SHIP, (101,55))
    lost.hp = 0
    lost.alive = False
    spare = unit(8001, UnitKind.TRANSPORT_SHIP, (102,55))
    ctx.world._units += [lost, spare]
    exp = Expedition(ctx.home, (253,53), [2000], ctx.villagers[0].id,
                     phase='sail', transport=lost.id, started=ctx.tick, loaded=True,
                     passengers=[2000,ctx.villagers[0].id], departure=(101,55))
    ctx.board.expedition = exp
    manager.step(ctx)
    assert ctx.board.expedition is None
    assert not ctx.client.named('unload')


def test_missing_forward_foundation_is_rebuilt_after_its_plan_expires():
    ctx, manager = campaign()
    exp = Expedition(ctx.home, (253,53), [2000], ctx.villagers[0].id,
                     phase='outpost', target=9000, started=ctx.tick,
                     sites={BuildingKind.BARRACKS:(220,55)})
    ctx.board.expedition = exp
    for u in ctx.world._units:
        if u.id in exp.crew or u.id == exp.builder:
            u.tile = (220,55)
    manager.step(ctx)
    assert ctx.client.named('build')[0][1] == BuildingKind.BARRACKS


def test_army_waits_for_its_bridge_construction_before_starting_a_ferry():
    ctx, manager = campaign()
    for u in ctx.army:
        u.tile = (99,55)
    manager.prepare(ctx)
    exp = ctx.board.expedition
    exp.phase = 'advance'
    exp.bridge = ((99,55),(110,55))
    ctx.buildings.append(building(5000, BuildingKind.BRIDGE, (100,55), complete=False))
    ctx.world.land_route = lambda *a: None
    ctx.world.land_frontier = lambda *a: None
    manager.step(ctx)
    assert exp.phase == 'advance'
    assert not ctx.client.named('load')


def test_bridge_evidence_requires_the_far_bank_after_the_deck():
    ctx, manager = campaign()
    manager.prepare(ctx)
    exp = ctx.board.expedition
    exp.bridge = ((99,55),(102,55))
    soldier = ctx.world.unit(exp.crew[0])
    soldier.tile = (100,55)
    manager.prepare(ctx)
    assert exp.crossed is None
    soldier.tile = (102,55)
    manager.prepare(ctx)
    assert exp.crossed == 'bridge'


def test_existing_forward_producers_can_reinforce_a_small_landing_force():
    ctx, manager = campaign()
    exp = Expedition(ctx.home, (253,53), [2000], ctx.villagers[0].id,
                     phase='outpost', target=9000, started=ctx.tick)
    ctx.board.expedition = exp
    for u in ctx.world._units:
        if u.id in exp.crew or u.id == exp.builder:
            u.tile = (220,55)
    ctx.buildings += [building(5000,BuildingKind.BARRACKS,(220,55)),
                      building(5001,BuildingKind.SIEGE_WORKSHOP,(224,55))]
    manager.step(ctx)
    assert ctx.client.named('train'), 'a landing must grow even when production already stands'


def test_ferry_does_not_unload_back_on_proven_home_land():
    ctx, manager = campaign()
    ship = unit(8000,UnitKind.TRANSPORT_SHIP,(111,55))
    ship.cargo_count = 5
    ship.auto_scout = True
    ctx.world._units.append(ship)
    exp = Expedition(ctx.home,(253,53),[2000,2001],ctx.villagers[0].id,
                     phase='sail',transport=ship.id,started=ctx.tick,loaded=True,
                     passengers=[2000,2001,ctx.villagers[0].id],departure=(101,55))
    ctx.board.expedition = exp
    ctx.world.passable = lambda x,y: x == 113
    ctx.world.water_passable = lambda x,y: x == 112
    ctx.world.land_route = lambda x,y,tx,ty: [(x,y),(tx,ty)]
    manager.step(ctx)
    assert not ctx.client.named('unload')
    assert not ctx.client.named('move'), 'do not cancel native exploration with a hop back to the same bay'


def test_reinforcements_are_reserved_from_spawn_events_without_rescanning_the_army():
    from types import SimpleNamespace
    ctx, manager = campaign()
    exp = Expedition(ctx.home,(253,53),[2000],ctx.villagers[0].id,
                     phase='attack',target=9000,started=ctx.tick)
    ctx.board.expedition = exp
    ctx.world.unit(2001).tile = (220,55)
    manager.prepare(ctx)
    assert 2001 not in exp.crew
    ctx.events = [SimpleNamespace(kind='unit_spawned',mine=True,id=2001)]
    manager.observe(ctx)
    manager.prepare(ctx)
    assert 2001 in exp.crew and 2001 in ctx.busy


def test_forward_builder_can_replace_an_army_that_has_just_been_lost():
    ctx, manager = campaign()
    exp = Expedition(ctx.home,(253,53),[],ctx.villagers[0].id,
                     phase='attack',target=9000,started=ctx.tick)
    ctx.board.expedition = exp
    ctx.world.unit(exp.builder).tile = (220,55)
    ctx.buildings += [building(5000,BuildingKind.BARRACKS,(220,55)),
                      building(5001,BuildingKind.SIEGE_WORKSHOP,(224,55))]
    manager.step(ctx)
    assert ctx.board.expedition is exp and ctx.client.named('train')


def test_arriving_at_an_empty_exploration_goal_continues_the_search_for_enemy_buildings():
    ctx, manager = campaign()
    ctx.world._enemy_buildings = []
    for u in ctx.army:
        u.tile = (150,150)
    exp = Expedition(ctx.home,(150,150),[2000,2001],ctx.villagers[0].id,
                     phase='advance',started=ctx.tick)
    ctx.board.expedition = exp
    manager.step(ctx)
    assert exp.goal != (150,150) and ctx.client.named('formation_move')

"""Defences protect working settlements and leave an owned gate through lanes."""
from aoeworld import Age, BuildingKind, building_cost

from aoe_ai.play.constructor import Constructor
from aoe_ai.play.fortification import FortificationManager, frontier
from aoe_ai.play.management import ManagementManager
from fakes import building, unit
from test_kingdom import kingdom


def resource_site(ctx):
    from aoeworld import UnitKind
    ctx.enemies = [unit(998, UnitKind.MILITIA, tile=(85, 55), mine=False)]
    ctx.buildings.append(building(900, BuildingKind.LUMBER_CAMP, tile=(76, 52)))
    return ctx.buildings[-1]


def test_resource_tower_precedes_optional_spending_and_is_not_duplicated():
    ctx = kingdom(villagers=40, centers=1)
    camp = resource_site(ctx)
    manager = FortificationManager(Constructor())
    manager.step(ctx)
    orders = ctx.client.named('build')
    assert len(orders) == 1 and orders[0][1] == BuildingKind.WATCH_TOWER
    assert abs(orders[0][2] - camp.center[0]) < 12
    ctx.tick += ctx.world.tick_hz * 60
    manager.step(ctx)
    assert len(ctx.client.named('build')) == 1, 'pending tower already protects this camp'


def test_low_income_and_age_reserves_keep_defences_from_spending():
    for changes in (dict(wood=80), dict(stone=200), dict(age=Age.DARK)):
        ctx = kingdom(villagers=40, centers=1, **changes)
        resource_site(ctx)
        FortificationManager(Constructor()).step(ctx)
        assert not ctx.client.named('build')


def test_more_workers_fund_more_resource_towers():
    for workers, wanted in ((30, 1), (120, 4)):
        ctx = kingdom(villagers=workers, centers=4)
        for i in range(wanted):
            ctx.buildings.append(building(900 + i, BuildingKind.WATCH_TOWER, tile=(91, 52 + i)))
        resource_site(ctx)
        FortificationManager(Constructor()).step(ctx)
        assert not ctx.client.named('build'), 'towers serving the threatened site count against its cap and the income ceiling'


def enemy(ctx, tile=(220, 90)):
    ctx.world._enemy_buildings = [building(999, BuildingKind.TOWN_CENTER, tile=tile, mine=False)]


def exact_sites(ctx):
    ctx.world.find_build_site = lambda kind, x, y, *args: (x, y)


def test_known_enemy_lane_gets_gate_before_walls_and_keeps_its_opening():
    ctx = kingdom(villagers=60, centers=1)
    enemy(ctx)
    exact_sites(ctx)
    manager = FortificationManager(Constructor())
    # A tower at the lane lets this decision concentrate on the gate.
    lane = manager.lane(ctx, ctx.home)
    ctx.buildings.append(building(900, BuildingKind.WATCH_TOWER, tile=lane))
    manager.step(ctx)
    gate = ctx.client.named('build')[0]
    assert gate[1] == BuildingKind.STONE_GATE
    ctx.buildings.append(building(901, gate[1], tile=gate[2:]))
    ctx.board.plans.clear()
    ctx.busy.clear()
    ctx.tick += 1200
    manager.step(ctx)
    wall = ctx.client.named('build')[-1]
    assert wall[1] == BuildingKind.STONE_WALL
    assert wall[2:] not in [(gate[2] + i, gate[3]) for i in range(4)]
    assert len(ctx.client.named('build')) == 2


def test_unknown_enemy_does_not_get_invented_approach_walls():
    ctx = kingdom(villagers=60, centers=1)
    FortificationManager(Constructor()).step(ctx)
    assert not ctx.client.named('build')


def test_unknown_or_unreachable_lane_never_receives_defensive_orders():
    ctx = kingdom(villagers=60, centers=1)
    enemy(ctx)
    ctx.world.land_route = lambda *args: None
    FortificationManager(Constructor()).step(ctx)
    assert not ctx.client.named('build')


def test_frontier_castle_follows_enemy_facing_settlement():
    ctx = kingdom(villagers=120, centers=3)
    enemy(ctx, (300, 50))
    point = frontier(ctx)
    assert point[0] > max(c[0] for c in ctx.centers)
    ManagementManager().castles(ctx)
    castle = ctx.client.named('build')[0]
    assert castle[1] == BuildingKind.CASTLE
    assert castle[2] > max(c[0] for c in ctx.centers)


def test_exact_wall_footprints_are_never_shifted_into_gate_or_worker_lanes():
    ctx = kingdom(villagers=60, centers=1)
    enemy(ctx)
    lane = FortificationManager(Constructor()).lane(ctx, ctx.home)
    ctx.buildings.append(building(900, BuildingKind.WATCH_TOWER, tile=lane))
    FortificationManager(Constructor()).step(ctx)
    assert not ctx.client.named('build'), 'relocated site cannot preserve wall geometry'


def test_terrain_narrows_the_selected_known_enemy_approach():
    ctx = kingdom(villagers=60, centers=1)
    enemy(ctx, (52, 250))
    # A gate-wide (four-tile) opening south of home is preferable to a broad open lane.
    narrow_y = ctx.home[1] + 18
    ctx.world.passable = lambda x, y: not (
        y == narrow_y and not ctx.home[0] - 2 <= x <= ctx.home[0] + 1)
    lane = FortificationManager(Constructor()).lane(ctx, ctx.home)
    assert lane[1] == narrow_y


def test_unexplored_terrain_does_not_masquerade_as_a_chokepoint():
    ctx = kingdom(villagers=60, centers=1)
    enemy(ctx, (52, 250))
    ctx.world.passable = lambda x, y: None
    assert FortificationManager(Constructor()).lane(ctx, ctx.home) is None


def test_refused_gate_site_is_not_sent_again_and_queries_stay_bounded():
    ctx = kingdom(villagers=60, centers=1)
    enemy(ctx)
    exact_sites(ctx)
    manager = FortificationManager(Constructor())
    lane = manager.lane(ctx, ctx.home)
    ctx.buildings.append(building(900, BuildingKind.WATCH_TOWER, tile=lane))
    manager.step(ctx)
    args = ctx.client.named('build')[0]
    ctx.board.bad_sites.add((str(args[1]), args[2], args[3]))
    ctx.board.plans.clear()
    ctx.tick += 1200
    ctx.busy.clear()
    manager.step(ctx)
    assert len(ctx.client.named('build')) == 1
    assert ctx._site_queries <= ctx.config.site_queries_per_decision


def test_approach_tower_leaves_gate_and_wall_footprints_free():
    ctx = kingdom(villagers=60, centers=1)
    enemy(ctx)
    exact_sites(ctx)
    manager = FortificationManager(Constructor())
    lane = manager.lane(ctx, ctx.home)
    manager.step(ctx)
    tower = ctx.client.named('build')[0]
    assert tower[1] == BuildingKind.WATCH_TOWER
    # Tower is behind the wall, clear of both its four-tile gate and wings.
    assert abs(tower[2] - lane[0]) >= 4 or abs(tower[3] - lane[1]) >= 4


def test_tower_can_use_expansion_float_while_preserving_house_money():
    ctx = kingdom(villagers=40, centers=1, wood=building_cost(BuildingKind.TOWN_CENTER)[1] - 25)
    resource_site(ctx)
    ManagementManager().expand(ctx)
    FortificationManager(Constructor()).step(ctx)
    assert any(a[1] == BuildingKind.WATCH_TOWER for a in ctx.client.named('build'))
    assert ctx.economy.wood - ctx.spent[1] >= ctx.config.wood_reserve


def test_a_tree_at_one_approach_corner_does_not_block_an_otherwise_reachable_tower():
    ctx = kingdom(villagers=40, centers=1)
    camp = resource_site(ctx)
    exact_sites(ctx)
    # North-west is wooded; a worker can still reach the east footprint edge.
    blocked = (camp.center[0] - 1, camp.center[1] - 1)
    ctx.world.land_route = lambda x, y, tx, ty: None if (tx, ty) == blocked else [(x, y), (tx, ty)]
    FortificationManager(Constructor()).step(ctx)
    assert any(a[1] == BuildingKind.WATCH_TOWER for a in ctx.client.named('build'))


def test_vassal_defences_do_not_wait_for_every_future_castle():
    from aoeworld import UnitKind
    ctx = kingdom(villagers=120, centers=5)
    castle = building(950, BuildingKind.CASTLE, tile=(65, 65))
    castle.garrison = [(951, UnitKind.VASSAL, 50, 50)]
    ctx.buildings.append(castle)
    enemy(ctx, (130, 80))
    # Native Vassals already preserve 200 food/wood/stone and 100 gold.
    # A growing castle target must not prevent all defensive development.
    ManagementManager().settings(ctx)
    assert ctx.client.named('vassal_development') == [(951, True)]
    ctx.config.fortifications = False
    ManagementManager().settings(ctx)
    ManagementManager().settings(ctx)
    assert ctx.client.named('vassal_development') == [(951, True), (951, False)]


def test_chokepoint_too_narrow_for_the_gate_is_rejected_for_another_cross_section():
    ctx = kingdom(villagers=60, centers=1)
    enemy(ctx, (52, 250))
    manager = FortificationManager(Constructor())
    x0 = ctx.home[0]
    # Every cross-section at 18 tiles has only three open tiles: a four-tile gate cannot fit.
    narrow = ctx.home[1] + 18
    ctx.world.passable = lambda x, y: not (y == narrow and not x0 - 1 <= x <= x0 + 1)
    lane = manager.lane(ctx, ctx.home)
    assert lane is not None and lane[1] != narrow
    ctx.world.passable = lambda x, y: not (abs(y - ctx.home[1]) > 5 and not x0 - 1 <= x <= x0 + 1)
    assert manager.lane(ctx, ctx.home) is None, 'no sampled cross-section can hold the gate'


def test_obstructed_gate_footprint_drops_the_lane_and_next_lane_is_used():
    ctx = kingdom(villagers=60, centers=1)
    enemy(ctx)
    exact_sites(ctx)
    manager = FortificationManager(Constructor())
    first = manager.lane(ctx, ctx.home)
    ctx.buildings.append(building(900, BuildingKind.WATCH_TOWER, tile=first))
    # A tree blocks the first lane's gate footprint; the engine refuses it.
    refuse = (first[0] - 2, first[1])
    ctx.world.find_build_site = lambda kind, x, y, *a: None if (x, y) == refuse else (x, y)
    manager.step(ctx)
    assert not ctx.client.named('build')
    assert ctx.board.fort_lanes.get(ctx.home) is None
    ctx.tick += 1200
    manager.step(ctx)
    second = ctx.board.fort_lanes[ctx.home]
    assert second != first
    ctx.buildings.append(building(901, BuildingKind.WATCH_TOWER, tile=second))
    ctx.tick += 1200
    manager.step(ctx)
    gates = [a for a in ctx.client.named('build') if a[1] == BuildingKind.STONE_GATE]
    assert gates and gates[0][2:] == (second[0] - 2, second[1])


def test_every_lane_refused_stops_after_the_sampled_candidates():
    ctx = kingdom(villagers=60, centers=1)
    enemy(ctx)
    ctx.world.find_build_site = lambda *a: None
    manager = FortificationManager(Constructor())
    for _ in range(12):
        ctx.tick += 1200
        ctx.board.plans.clear()
        manager.step(ctx)
        lane = ctx.board.fort_lanes.get(ctx.home)
        if lane is not None:
            ctx.buildings.append(building(900 + len(ctx.buildings), BuildingKind.WATCH_TOWER, tile=lane))
    assert len(ctx.board.fort_rejected[ctx.home]) <= len(ctx.config.fort_lane_distances)
    assert not [a for a in ctx.client.named('build') if a[1] == BuildingKind.STONE_GATE]


def test_castle_across_water_falls_back_to_a_reachable_settlement_site():
    ctx = kingdom(villagers=120, centers=3)
    enemy(ctx, (300, 50))
    far = frontier(ctx)
    # East of every settlement is sea: no builder can walk to anything there.
    limit = max(c[0] for c in ctx.centers)
    real = ctx.world.land_route
    ctx.world.land_route = lambda x, y, tx, ty: None if tx > limit + 1 else real(x, y, tx, ty)
    ctx.world.find_build_site = lambda kind, x, y, *a: (x, y)
    ManagementManager().castles(ctx)
    castle = ctx.client.named('build')[0]
    assert castle[1] == BuildingKind.CASTLE
    assert castle[2] <= limit + 1 and castle[2] < far[0]


def gate_ready(ctx):
    enemy(ctx)
    exact_sites(ctx)
    manager = FortificationManager(Constructor())
    lane = manager.lane(ctx, ctx.home)
    ctx.buildings.append(building(900, BuildingKind.WATCH_TOWER, tile=lane))
    ctx.board.fort_lanes[ctx.home] = lane
    return manager, lane


def test_exhausted_land_budget_is_not_an_unreachable_gate_and_keeps_the_lane():
    ctx = kingdom(villagers=60, centers=1)
    manager, lane = gate_ready(ctx)
    ctx._land_queries = ctx.config.land_queries_per_decision
    manager.step(ctx)
    assert not ctx.client.named('build')
    assert ctx.board.fort_lanes[ctx.home] == lane and not ctx.board.fort_rejected
    ctx._land_queries = 0
    ctx.tick += 1200
    manager.step(ctx)
    assert ctx.client.named('build')[0][1] == BuildingKind.STONE_GATE, 'retried once answerable'


def test_answered_unreachable_gate_edges_do_reject_the_lane():
    ctx = kingdom(villagers=60, centers=1)
    manager, lane = gate_ready(ctx)
    ctx.world.land_route = lambda *args: None
    manager.step(ctx)
    assert ctx.board.fort_rejected[ctx.home] == {lane}
    assert ctx.board.fort_lanes.get(ctx.home) is None


def test_lane_choice_waits_for_land_budget_instead_of_ranking_partial_answers():
    ctx = kingdom(villagers=60, centers=1)
    enemy(ctx)
    ctx._land_queries = ctx.config.land_queries_per_decision
    assert FortificationManager(Constructor()).lane(ctx, ctx.home) is None
    ctx._land_queries = 0
    assert FortificationManager(Constructor()).lane(ctx, ctx.home) is not None


def test_castle_waits_for_land_budget_instead_of_falling_back_from_unanswered_frontier():
    ctx = kingdom(villagers=120, centers=3)
    enemy(ctx, (300, 50))
    ctx.world.find_build_site = lambda kind, x, y, *a: (x, y)
    ctx._land_queries = ctx.config.land_queries_per_decision
    ManagementManager().castles(ctx)
    assert not ctx.client.named('build')
    ctx._land_queries = 0
    ctx.tick += 10 * ctx.world.tick_hz + 1
    ManagementManager().castles(ctx)
    assert ctx.client.named('build')[0][2] > max(c[0] for c in ctx.centers)

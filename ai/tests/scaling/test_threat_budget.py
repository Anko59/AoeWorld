"""A peaceful settlement must spend its workforce and savings on growth."""
from types import SimpleNamespace

from aoeworld import Age, BuildingKind, ResourceKind, Tech, UnitKind, UnitTask

from aoe_ai.brain import Brain
from aoe_ai.context import Context
from aoe_ai.play.constructor import Constructor
from aoe_ai.play.economy import EconomyManager
from aoe_ai.play.fortification import FortificationManager
from aoe_ai.play.management import ManagementManager, funding_reserve
from aoe_ai.play.naval import NavalManager
from aoe_ai.play.research import ResearchManager
from fakes import FakeClient, building, decision, node, unit
from test_kingdom import kingdom


def test_wood_starved_feudal_economy_does_not_mine_for_an_unavailable_castle():
    ctx = kingdom(villagers=29, centers=1, age=Age.FEUDAL, food=2000, wood=9, stone=200)
    shares = EconomyManager(Constructor()).shares(ctx)
    assert shares['STONE'] == 0, 'Castle stone must not compete with the Feudal army and age prerequisites'
    assert shares['WOOD'] >= ctx.config.feudal_shares['WOOD']
    ctx.economy.age = Age.CASTLE
    assert EconomyManager(Constructor()).shares(ctx)['STONE'] > 0, 'unlocked Castles still receive funding'


def test_exhausted_start_can_gather_known_timber_beyond_its_settlement_radius():
    ctx = kingdom(villagers=29, centers=1, age=Age.FEUDAL, food=2000, wood=9)
    tree = node(999, ResourceKind.WOOD, (180, 55))
    ctx.world._nodes = [tree]
    worker = ctx.villagers[0]
    assert EconomyManager(Constructor()).send(ctx, worker, 'WOOD')
    assert ctx.client.named('gather') == [(worker.id, tree.id)], 'long returns can fund a first forest camp'


def test_remote_cutter_funds_a_dropoff_at_its_forest_without_more_route_queries():
    ctx = kingdom(villagers=29, centers=1, wood=150)
    tree = node(999, ResourceKind.WOOD, (180, 55))
    ctx.world._nodes = [tree]
    worker = ctx.villagers[0]
    worker.tile, worker.task, worker.gathering = (179, 55), UnitTask.GATHERING, ResourceKind.WOOD
    EconomyManager(Constructor()).wood_dropoff(ctx)
    assert any(a[1] == BuildingKind.LUMBER_CAMP for a in ctx.client.named('build'))
    assert ctx._land_queries == 0


def test_peaceful_fishing_dock_does_not_buy_defensive_galleys():
    ctx = kingdom(villagers=29, centers=1, age=Age.FEUDAL)
    ctx.buildings.append(building(900, BuildingKind.DOCK, tile=(65, 60)))
    ctx.units.append(unit(901, UnitKind.FISHING_SHIP, tile=(65, 61)))
    ctx.world._nodes = [node(999, ResourceKind.FOOD, (66, 62))]
    ctx.world.passable = lambda x, y: False
    NavalManager(Constructor()).step(ctx)
    assert not ctx.client.named('train'), 'unthreatened water is an economy investment, not a war fleet'


def test_optional_imperial_gold_shortage_does_not_freeze_all_town_center_growth():
    technologies = [getattr(Tech, n) for n in dir(Tech) if n[0].isupper()]
    ctx = kingdom(villagers=80, centers=4, age=Age.IMPERIAL, food=8000, wood=8000, gold=0,
                  researched=[t for t in technologies if t != Tech.BRACER])
    ctx.config.save_for_management = False
    ResearchManager().step(ctx)
    assert ctx.saving_castle and ctx.draining, 'army spending still yields to the gold reserve'
    assert not ctx.hold_town_center, 'an unfunded optional upgrade must not freeze every settlement'
    EconomyManager(Constructor()).train_villagers(ctx)
    assert any(a[1] == UnitKind.VILLAGER for a in ctx.client.named('train'))


def test_optional_imperial_food_shortage_still_allows_wood_only_farms():
    technologies = [getattr(Tech, n) for n in dir(Tech) if n[0].isupper()]
    ctx = kingdom(villagers=80, centers=4, age=Age.IMPERIAL, food=0, wood=8000, gold=8000,
                  researched=[t for t in technologies if t != Tech.BRACER])
    ctx.config.save_for_management = False
    ctx.buildings.append(building(900, BuildingKind.MILL, tile=(55, 55)))
    ResearchManager().step(ctx)
    assert EconomyManager(Constructor()).send_to_farm(ctx, ctx.villagers[0])
    assert any(a[1] == BuildingKind.FARM for a in ctx.client.named('build'))


def seated_vassal(ctx):
    castle = building(950, BuildingKind.CASTLE, tile=(65, 65))
    castle.garrison = [(951, UnitKind.VASSAL, 50, 50)]
    ctx.buildings.append(castle)
    return castle


def test_peaceful_resource_site_and_distant_enemy_do_not_justify_towers():
    for enemies in ([], [building(999, BuildingKind.TOWN_CENTER, tile=(800, 800), mine=False)]):
        ctx = kingdom(villagers=120, centers=1)
        ctx.buildings.append(building(900, BuildingKind.LUMBER_CAMP, tile=(76, 52)))
        ctx.world._enemy_buildings = enemies
        FortificationManager(Constructor()).step(ctx)
        assert not ctx.client.named('build'), 'income alone must not authorize a tower'


def test_peaceful_vassal_stops_development_and_releases_reserved_workers():
    ctx = kingdom(villagers=60, centers=1)
    castle = seated_vassal(ctx)
    workers = {v.id for v in ctx.villagers[:8]}
    ctx.board.vassal_workers[castle.id] = workers
    ctx.reserved_workers.update(workers)
    ctx.board.manager_settings[951] = (True,)
    manager = ManagementManager()
    manager.settings(ctx)
    manager.defenders(ctx)
    assert ctx.client.named('vassal_development') == [(951, False)]
    assert not ctx.reserved_workers & workers
    assert not ctx.board.vassal_workers
    assert not ctx.client.named('move')


def test_peaceful_castle_recruits_general_without_saving_for_a_vassal():
    ctx = kingdom(villagers=35, centers=1)
    ctx.buildings.append(building(950, BuildingKind.CASTLE, tile=(65, 65)))
    assert funding_reserve(ctx) == (800, 0, 800, 0), 'science must not fund an unneeded Vassal'
    ManagementManager().recruit(ctx)
    assert any(a[1] == UnitKind.GENERAL for a in ctx.client.named('train'))
    assert funding_reserve(ctx) == (0, 0, 0, 0), 'the ordered General has already been paid'


def test_nearby_enemy_enables_only_the_frontier_castles_vassal():
    ctx = kingdom(villagers=180, centers=1)
    first = seated_vassal(ctx)
    rear = building(960, BuildingKind.CASTLE, tile=(700, 700))
    rear.garrison = [(961, UnitKind.VASSAL, 50, 50)]
    ctx.buildings.append(rear)
    ctx.world._enemy_buildings = [building(999, BuildingKind.BARRACKS, tile=(130, 80), mine=False)]
    ManagementManager().settings(ctx)
    assert ctx.client.named('vassal_development') == [(951, True), (961, False)]
    ManagementManager().defenders(ctx)
    assert first.id in ctx.board.vassal_workers and rear.id not in ctx.board.vassal_workers


def test_vassal_in_a_quiet_rear_castle_does_not_fill_a_threatened_frontier_seat():
    ctx = kingdom(villagers=35, centers=1)
    ctx.config.stewards = ctx.config.generals = 0
    rear = building(950, BuildingKind.CASTLE, tile=(700, 700))
    rear.garrison = [(951, UnitKind.VASSAL, 50, 50)]
    ctx.buildings.extend([rear, building(960, BuildingKind.CASTLE, tile=(65, 65))])
    ctx.world._enemy_buildings = [building(999, BuildingKind.BARRACKS, tile=(130, 80), mine=False)]
    assert funding_reserve(ctx)[0] > 0, 'the threatened seat still needs funding'
    ManagementManager().recruit(ctx)
    assert ctx.client.named('train') == [(960, UnitKind.VASSAL)]


def test_local_loss_allows_defences_then_expires_without_funding_remote_losses():
    for tile, mine, expected in (((75, 55), True, True), ((800, 800), True, False), ((75, 55), False, False)):
        ctx = kingdom(villagers=40, centers=1)
        ctx.buildings.append(building(900, BuildingKind.LUMBER_CAMP, tile=(76, 52)))
        ctx.events = [SimpleNamespace(kind='unit_died', mine=mine, x=tile[0], y=tile[1])]
        Brain().absorb(ctx)
        FortificationManager(Constructor()).step(ctx)
        assert bool(ctx.client.named('build')) is expected
        ctx.board.plans.clear()
        ctx.world.tick += 20 * 60 * ctx.world.tick_hz
        ctx = Context(FakeClient(), decision(ctx.world), ctx.board, ctx.config)
        FortificationManager(Constructor()).step(ctx)
        assert not ctx.client.named('build'), 'old losses cannot fund permanent spending'


def test_visible_raider_protects_resource_work_without_a_known_enemy_building():
    ctx = kingdom(villagers=40, centers=1)
    ctx.buildings.append(building(900, BuildingKind.LUMBER_CAMP, tile=(76, 52)))
    ctx.enemies = [unit(999, UnitKind.MILITIA, tile=(85, 55), mine=False)]
    FortificationManager(Constructor()).step(ctx)
    assert any(a[1] == BuildingKind.WATCH_TOWER for a in ctx.client.named('build'))


def test_lost_own_building_uses_its_last_observed_position():
    ctx = kingdom(villagers=40, centers=1)
    camp = building(900, BuildingKind.LUMBER_CAMP, tile=(76, 52))
    ctx.buildings.append(camp)
    brain = Brain()
    brain.absorb(ctx)
    ctx.buildings.remove(camp)
    ctx.events = [SimpleNamespace(kind='building_destroyed', id=camp.id, mine=True)]
    brain.absorb(ctx)
    seated_vassal(ctx)
    ManagementManager().settings(ctx)
    assert ctx.client.named('vassal_development') == [(951, True)]


def test_damage_without_a_loss_funds_local_defences_and_alert_memory_is_bounded():
    ctx = kingdom(villagers=40, centers=1)
    seated_vassal(ctx)
    ctx.events = [SimpleNamespace(kind='under_attack', x=i * 8, y=60) for i in range(1000)]
    ctx.events.append(SimpleNamespace(kind='under_attack', x=70, y=60))
    Brain().absorb(ctx)
    assert len(ctx.board.defence_alerts) <= ctx.config.fort_alert_limit
    ManagementManager().settings(ctx)
    assert ctx.client.named('vassal_development') == [(951, True)]


def test_remote_camp_under_attack_gets_its_own_tower_without_a_threatened_town_center():
    ctx = kingdom(villagers=60, centers=1)
    ctx.buildings.append(building(900, BuildingKind.LUMBER_CAMP, tile=(250, 52)))
    ctx.enemies = [unit(999, UnitKind.MILITIA, tile=(255, 55), mine=False)]
    FortificationManager(Constructor()).step(ctx)
    towers = [a for a in ctx.client.named('build') if a[1] == BuildingKind.WATCH_TOWER]
    assert towers and abs(towers[0][2] - 250) < 15, 'a threatened remote site authorises its own tower'


def test_newly_threatened_settlement_builds_though_a_quiet_one_holds_towers():
    ctx = kingdom(villagers=120, centers=3)
    quiet = ctx.centers[0]
    ctx.buildings += [building(800 + i, BuildingKind.WATCH_TOWER, tile=(quiet[0] + 6 + 2 * i, quiet[1]))
                      for i in range(4)]
    far = max(ctx.centers)
    ctx.enemies = [unit(999, UnitKind.MILITIA, tile=(far[0] + 25, far[1] + 5), mine=False)]
    ctx.world._enemy_buildings = [building(998, BuildingKind.BARRACKS, tile=(far[0] + 60, far[1]), mine=False)]
    FortificationManager(Constructor()).step(ctx)
    towers = [a for a in ctx.client.named('build') if a[1] == BuildingKind.WATCH_TOWER]
    assert towers, 'towers at a quiet settlement must not consume the threatened one\'s allowance'


def test_threatened_site_stops_at_its_own_cap():
    ctx = kingdom(villagers=300, centers=1)
    ctx.buildings.append(building(900, BuildingKind.LUMBER_CAMP, tile=(250, 52)))
    ctx.buildings += [building(800 + i, BuildingKind.WATCH_TOWER, tile=(246 + 8 * i, 52)) for i in range(2)]
    ctx.enemies = [unit(999, UnitKind.MILITIA, tile=(255, 55), mine=False)]
    FortificationManager(Constructor()).step(ctx)
    assert not ctx.client.named('build'), 'a site never exceeds its per-site tower cap'


def overlapping_sites(towers, villagers=120):
    """Threatened centre at the origin, threatened camp centred 25 tiles east of it."""
    ctx = kingdom(villagers=villagers, centers=1)
    cx, cy = ctx.centers[0]
    # Tiles are chosen so the 2x2 footprints are CENTRED on the stated offsets.
    camp = building(900, BuildingKind.LUMBER_CAMP, tile=(cx + 24, cy - 1))
    ctx.buildings.append(camp)
    ctx.buildings += [building(800 + i, BuildingKind.WATCH_TOWER, tile=(cx + dx - 1, cy + dy - 1))
                      for i, (dx, dy) in enumerate(towers)]
    ctx.enemies = [unit(999, UnitKind.MILITIA, tile=(cx + 12, cy + 8), mine=False)]
    FortificationManager(Constructor()).step(ctx)
    return [a for a in ctx.client.named('build') if a[1] == BuildingKind.WATCH_TOWER]


def test_town_center_towers_on_its_far_side_do_not_deny_an_overlapping_camp():
    assert overlapping_sites([(-20, 0), (-22, 0)]), 'each tower is counted once, for the site it serves'


def test_camp_cannot_exceed_its_cap_when_its_reach_overlaps_a_threatened_center():
    assert not overlapping_sites([(42, 0), (25, 17)], villagers=90), 'two towers in the camp reach fill its cap'

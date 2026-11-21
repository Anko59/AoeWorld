"""An engagement must survive the rest of the same combat decision."""

from types import SimpleNamespace

import pytest
from aoeworld import Formation, BuildingKind, CommandResult, UnitKind, UnitTask

from aoe_ai import Brain
from aoe_ai.blackboard import Blackboard
from aoe_ai.config import Config
from aoe_ai.context import Context
from aoe_ai.play.combat import CombatManager
from aoe_ai.play.combat.march import march
from aoe_ai.play.military import MilitaryManager
from test_logic import play
from fakes import FakeClient, FakeWorld, base, building, decision, order_result, unit


def test_production_keeps_a_staging_rally_while_the_army_is_attacking():
    units, buildings = base(3)
    barracks = building(600, BuildingKind.BARRACKS, tile=(68, 53))
    barracks.rally_position = None
    world = FakeWorld(units, buildings + [barracks])
    board, client = Blackboard(attacking=True), FakeClient()
    manager = MilitaryManager()
    manager.rally(Context(client, decision(world), board, Config()))

    (structure, x, y), = client.named("set_rally")
    assert structure == barracks.id
    assert (x, y) != barracks.center
    assert world.land_route(*barracks.center, x, y)

    barracks.rally_position = (x, y)
    client = FakeClient()
    manager.rally(Context(client, decision(world), board, Config()))
    assert client.named("set_rally") == []


def test_a_scattered_roster_keeps_staging_even_above_the_attack_threshold():
    units, buildings = base(3)
    army = [unit(300 + i, UnitKind.SPEARMAN, tile=(60, 60)) for i in range(7)]
    army += [unit(400 + i, UnitKind.SPEARMAN, tile=(180, 150)) for i in range(7)]
    enemy = building(900, BuildingKind.HOUSE, tile=(90, 60), mine=False)
    world = FakeWorld(units + army, buildings, enemy_buildings=[enemy])
    board, client = Blackboard(), FakeClient()
    CombatManager().step(Context(client, decision(world), board, Config()))

    assert not board.attacking
    assert client.named("attack") == []
    assert client.named("formation_move") == []


def test_a_staged_force_attacks_the_nearest_reachable_target_without_stragglers():
    units, buildings = base(3)
    staged = [unit(300 + i, UnitKind.SPEARMAN, tile=(60, 60)) for i in range(12)]
    far = [unit(400 + i, UnitKind.SPEARMAN, tile=(160, 170)) for i in range(9)]
    far += [unit(500 + i, UnitKind.SPEARMAN, tile=(220, 200)) for i in range(8)]
    blocked = building(900, BuildingKind.HOUSE, tile=(71, 59), mine=False)
    reachable = building(901, BuildingKind.HOUSE, tile=(85, 59), mine=False)
    distant = building(902, BuildingKind.HOUSE, tile=(160, 185), mine=False)
    world = FakeWorld(units + staged + far, buildings, enemy_buildings=[blocked, reachable, distant])
    land_route = world.land_route
    world.land_route = lambda x, y, tx, ty: None if (tx, ty) == blocked.center else land_route(x, y, tx, ty)
    board, client = Blackboard(), FakeClient()
    CombatManager().step(Context(client, decision(world), board, Config()))

    assert board.attack_target == reachable.id
    assert set(client.named("attack")) == {(soldier.id, reachable.id) for soldier in staged}
    assert client.named("formation_move") == []


@pytest.mark.parametrize("known_building", [False, True])
def test_nearby_enemy_engagement_is_not_overwritten_by_the_march(known_building):
    units, buildings = base(3)
    soldier = unit(300, UnitKind.SPEARMAN, tile=(120, 120))
    enemy = unit(800, UnitKind.VILLAGER, tile=(122, 120), mine=False)
    target = building(900, BuildingKind.HOUSE, tile=(150, 150), mine=False)
    world = FakeWorld(units + [soldier], buildings, enemies=[enemy],
                      enemy_buildings=[target] if known_building else [])
    client = FakeClient()
    board = Blackboard(attacking=True, attack_launch_size=1)
    ctx = Context(client, decision(world), board, Config())
    CombatManager().step(ctx)
    orders = [(name, args) for name, args in client.calls if args[0] == soldier.id]
    assert orders == [("attack", (soldier.id, enemy.id))]


def test_marching_soldier_engages_visible_enemy_before_reaching_search_destination():
    units, buildings = base(3)
    soldier = unit(300, UnitKind.SPEARMAN, tile=(120, 120), task=UnitTask.MOVING)
    soldier.moving = True
    enemy = unit(800, UnitKind.VILLAGER, tile=(122, 120), mine=False)
    world = FakeWorld(units + [soldier], buildings, enemies=[enemy])
    client = FakeClient()
    board = Blackboard(attacking=True, attack_launch_size=1)
    ctx = Context(client, decision(world), board, Config())
    CombatManager().step(ctx)
    assert client.named("attack") == [(soldier.id, enemy.id)]


def test_nearby_fogged_building_is_not_assumed_destroyed():
    units, buildings = base(3)
    soldier = unit(300, UnitKind.SPEARMAN, tile=(120, 120))
    target = building(900, BuildingKind.HOUSE, tile=(120, 124), mine=False, visible=False)
    world = FakeWorld(units + [soldier], buildings, enemy_buildings=[target])
    client = FakeClient()
    board = Blackboard(attacking=True, attack_launch_size=1)
    ctx = Context(client, decision(world), board, Config())
    CombatManager().step(ctx)
    assert board.attack_target == target.id
    assert client.named("move")
    assert board.stats.get("targets_cleared", 0) == 0


@pytest.mark.parametrize("result", [CommandResult.REJECTED_UNREACHABLE,
                                   CommandResult.REJECTED_INVALID_TARGET,
                                   CommandResult.REJECTED_PATH_BUDGET_EXCEEDED])
def test_refused_attack_retargets_without_blacklisting_other_soldiers(result):
    units, buildings = base(3)
    soldiers = [unit(300 + i, UnitKind.SPEARMAN, tile=(120, 120)) for i in range(2)]
    near = building(900, BuildingKind.HOUSE, tile=(130, 124), mine=False)
    other = building(901, BuildingKind.HOUSE, tile=(140, 124), mine=False)
    world = FakeWorld(units + soldiers, buildings, enemy_buildings=[near, other])
    brain, client = Brain(), FakeClient()
    brain.board.attacking, brain.board.attack_launch_size = True, 2
    ctx = Context(client, decision(world), brain.board, brain.config)
    brain.combat.step(ctx)
    sequence = next(i + 1 for i, call in enumerate(client.calls)
                    if call == ("attack", (soldiers[0].id, near.id)))
    refusal = order_result(sequence, result)
    client = FakeClient()
    ctx = Context(client, decision(world, [refusal]), brain.board, brain.config)
    brain.absorb(ctx)
    brain.combat.step(ctx)
    assert (soldiers[0].id, other.id) in client.named("attack")
    assert (soldiers[1].id, near.id) in client.named("attack")


def test_army_keeps_unseen_town_center_until_the_server_forgets_it():
    units, buildings = base(10)
    # Footprint [98,102) x [98,102): (106,100) is five tiles from
    # its nearest tile, beyond a spearman's four-tile sight.
    army = [unit(200 + i, UnitKind.SPEARMAN, tile=(106, 100)) for i in range(15)]
    target = building(901, BuildingKind.TOWN_CENTER, tile=(98, 98), mine=False, visible=False)
    brain, client = play(FakeWorld(units + army, buildings, enemy_buildings=[target]))
    assert brain.board.attack_target == target.id
    assert client.named("move")
    target.visible = True
    _, client = play(FakeWorld(units + army, buildings, enemy_buildings=[target]), brain=brain)
    assert any(args[1] == target.id for args in client.named("attack"))
    # Destruction/empty-footprint inspection is authoritative fog memory.
    _, client = play(FakeWorld(units + army, buildings), brain=brain)
    assert not client.named("attack")
    assert client.named("move")


def test_two_defenders_still_pull_workers_back_from_ten_raiders():
    units, buildings = base(6)
    worker = units[0]
    worker.tile = (68, 52)
    worker.x, worker.y = worker.tile
    worker.task, worker.idle = UnitTask.GATHERING, False
    army = [unit(300 + i, UnitKind.SPEARMAN, tile=(55, 52)) for i in range(2)]
    # The endangered worker is beside the raiders beyond the four queried targets.
    raiders = [unit(800 + i, UnitKind.SPEARMAN, tile=(53 + i, 52), mine=False)
               for i in range(10)]
    world = FakeWorld(units + army, buildings, enemies=raiders)
    client, board, config = FakeClient(), Blackboard(), Config()
    ctx = Context(client, decision(world), board, config)
    CombatManager().defend(ctx, army)
    assert (worker.id, 56, 56) in client.named("move")
    assert client.named("attack"), "route budget must still leave room to defend"
    assert ctx._land_queries <= config.land_queries_per_decision // 4 + len(army)


def test_reported_raid_preserves_worker_retreat_when_the_army_is_outnumbered():
    units, buildings = base(6)
    worker = units[0]
    worker.tile = (68, 52)
    worker.x, worker.y = worker.tile
    worker.task, worker.idle = UnitTask.GATHERING, False
    defenders = [unit(301, UnitKind.SPEARMAN, tile=(55, 52))]
    raiders = [unit(800 + i, UnitKind.SPEARMAN, tile=(53 + i, 52), mine=False)
               for i in range(10)]
    event = SimpleNamespace(kind="under_attack", id=buildings[0].id, x=53, y=52,
                            is_building=True, attacker=raiders[0].id)
    client = FakeClient()
    world = FakeWorld(units + defenders, buildings, enemies=raiders)
    CombatManager().step(Context(client, decision(world, [event]), Blackboard(), Config()))

    assert (worker.id, 56, 56) in client.named("move")
    assert client.named("attack")


def test_unconnected_raiders_do_not_trigger_defence():
    units, buildings = base(6)
    raiders = [unit(800 + i, UnitKind.SPEARMAN, tile=(60, 52), mine=False)
               for i in range(10)]
    world = FakeWorld(units, buildings, enemies=raiders)
    world.land_route = lambda *args: None
    client = FakeClient()
    ctx = Context(client, decision(world), Blackboard(), Config())
    assert not CombatManager().defend(ctx, [])
    assert not client.calls


def test_marching_an_army_uses_one_formation_and_keeps_stragglers_local():
    army = [unit(300+i, UnitKind.SPEARMAN, tile=(60,60)) for i in range(14)]
    army.append(unit(400, UnitKind.SPEARMAN, tile=(5,60)))
    client = FakeClient()
    world = FakeWorld(army, tick=100)
    ctx = Context(client, decision(world), Blackboard(), Config())
    march(ctx, army, (60,60), (200,200))
    groups = client.named("formation_move")
    assert len(groups) == 1 and groups[0][0] == list(range(300,314))
    assert groups[0][3] == Formation.LINE
    # The march is an attack-move, so soldiers answer enemies they meet.
    assert client.options_of("formation_move") == [{"attack_move": True}]
    assert len(client.named("move")) == 1 and client.named("move")[0][0] == 400
    assert ctx.busy == {u.id for u in army}
    # The same idle report immediately afterwards must not reset the shared route.
    march(ctx, army, (60,60), (200,200))
    assert client.named("formation_move") == groups


def test_distributed_army_reserves_land_queries_for_its_shared_march():
    units, buildings = base(10)
    army = [unit(300+i, UnitKind.SPEARMAN, tile=(55+i%7,55+i//7)) for i in range(28)]
    target = building(901, BuildingKind.HOUSE, tile=(250,250), mine=False, visible=False)
    world = FakeWorld(units+army, buildings, enemy_buildings=[target])
    client = FakeClient()
    ctx = Context(client, decision(world), Blackboard(attacking=True, attack_launch_size=28), Config())
    CombatManager().attack(ctx, army)
    group, = client.named("formation_move")
    assert group[0] == list(range(300,328))
    assert ctx._land_queries <= ctx.config.land_queries_per_decision


@pytest.mark.parametrize("known_building", [False, True])
def test_stationary_moving_army_recovers_after_the_stall_deadline(known_building):
    units, buildings = base(3)
    army = [unit(300+i, UnitKind.SPEARMAN, tile=(60, 60)) for i in range(2)]
    target = building(901, BuildingKind.HOUSE, tile=(250, 250), mine=False, visible=False)
    board = Blackboard(attacking=True, attack_launch_size=len(army))
    config = Config()

    def step(tick):
        world = FakeWorld(units + army, buildings, tick=tick,
                          enemy_buildings=[target] if known_building else [])
        client = FakeClient()
        CombatManager().step(Context(client, decision(world), board, config))
        return client.calls

    assert step(100), "start an ordinary shared march"
    for soldier in army:
        soldier.task, soldier.idle, soldier.moving = UnitTask.MOVING, False, True
    assert step(100 + config.march_pause_seconds * 20 + 1) == []
    # Seed 3's accepted marches reported MOVING at identical tiles for thirteen
    # minutes. That task flag alone must not prevent bounded route recovery.
    deadline = 100 + config.march_stall_seconds * 20 + 1
    recovery = step(deadline) + step(deadline + config.march_pause_seconds * 20)
    assert recovery, "a stationary MOVING army must receive a recovery order"
    recovered = set()
    for name, args in recovery:
        if name == "formation_move":
            recovered.update(args[0])
        elif name in ("move", "stop"):
            recovered.add(args[0])
    assert recovered == {soldier.id for soldier in army}
    # Once the accepted stop is reflected in the snapshot, recovery must resume
    # the march rather than leave the army idle or abandon its remembered enemy.
    for soldier in army:
        soldier.task, soldier.idle, soldier.moving = UnitTask.IDLE, True, False
    resumed = step(deadline + config.march_pause_seconds * 20 + 10)
    marching = set()
    for name, args in resumed:
        if name == "formation_move":
            marching.update(args[0])
        elif name == "move":
            marching.add(args[0])
    assert marching == {soldier.id for soldier in army}
    if known_building:
        assert not board.cooling(f"target:{target.id}", deadline + config.march_pause_seconds * 20 + 10)
        assert board.attack_target == target.id


def test_a_moving_army_keeps_its_route_while_it_is_making_progress():
    units, buildings = base(3)
    army = [unit(300+i, UnitKind.SPEARMAN, tile=(60, 60)) for i in range(2)]
    board = Blackboard(attacking=True, attack_launch_size=len(army))
    config = Config()
    for index in range(4):
        world = FakeWorld(units + army, buildings,
                          tick=100 + index * (config.march_stall_seconds * 20 // 2))
        client = FakeClient()
        CombatManager().step(Context(client, decision(world), board, config))
        if index == 0:
            assert client.named("formation_move")
        else:
            assert client.calls == [], "progress must preserve the accepted route"
        for soldier in army:
            soldier.tile = (soldier.tile[0] + 10, soldier.tile[1] + 10)
            soldier.x, soldier.y = soldier.tile
            soldier.task, soldier.idle, soldier.moving = UnitTask.MOVING, False, True


def raid(soldiers, *, tick=100, raider_visible=True):
    workers, buildings = base(3)
    attacker = unit(901, UnitKind.MILITIA, tile=(55, 54), mine=False)
    attacker.owner = 2
    victim = buildings[0]
    hit = SimpleNamespace(kind="under_attack", id=victim.id, x=55, y=54,
                          is_building=True, attacker=attacker.id)
    return FakeWorld(workers + soldiers, buildings, tick=tick,
                     enemies=[attacker] if raider_visible else []), hit


def test_damage_to_a_town_center_recalls_a_distant_army_in_one_order():
    soldiers = [unit(300 + i, UnitKind.SPEARMAN, tile=(245, 220), task=UnitTask.MOVING)
                for i in range(12)]
    for soldier in soldiers:
        soldier.moving = True
        soldier.attack_target = 888
    world, hit = raid(soldiers)
    client = FakeClient()
    board = Blackboard(attacking=True, attack_launch_size=len(soldiers))
    ctx = Context(client, decision(world, [hit]), board, Config())
    CombatManager().step(ctx)

    (members, x, y, _), = client.named("formation_move")
    assert members == [u.id for u in soldiers]
    assert x < soldiers[0].x and y < soldiers[0].y
    assert client.options_of("formation_move") == [{"attack_move": False}], "disengage from the old target"


def test_a_small_idle_group_engages_a_visible_enemy_away_from_home():
    soldiers = [unit(300 + i, UnitKind.SPEARMAN, tile=(220 + i, 220)) for i in range(3)]
    workers, buildings = base(3)
    enemy = unit(800, UnitKind.MILITIA, tile=(221, 222), mine=False)
    world = FakeWorld(workers + soldiers, buildings, enemies=[enemy])
    client = FakeClient()
    CombatManager().step(Context(client, decision(world), Blackboard(), Config()))

    assert {args[0] for args in client.named("attack")} == {u.id for u in soldiers}


def test_repelled_raid_targets_the_attackers_base_over_a_closer_bystander():
    soldiers = [unit(300 + i, UnitKind.SPEARMAN, tile=(65, 60)) for i in range(15)]
    board, cfg = Blackboard(), Config()
    world, hit = raid(soldiers)
    CombatManager().step(Context(FakeClient(), decision(world, [hit]), board, cfg))

    world, _ = raid(soldiers, tick=world.tick + 20 * 90, raider_visible=False)
    bystander = building(880, BuildingKind.HOUSE, tile=(90, 65), mine=False)
    base = building(890, BuildingKind.TOWN_CENTER, tile=(175, 165), mine=False)
    base.owner = 2
    world._enemy_buildings = [bystander, base]
    client = FakeClient()
    CombatManager().step(Context(client, decision(world), board, cfg))

    assert board.attack_target == base.id
    assert client.named("formation_move")


def test_each_spread_out_march_group_uses_one_route_query_for_its_hop():
    army = [unit(300 + i, UnitKind.SPEARMAN, tile=(35 + 20 * i, 60))
            for i in range(8)]
    world = FakeWorld(army, size=(500, 500))
    client = FakeClient()
    ctx = Context(client, decision(world), Blackboard(), Config(land_queries_per_decision=8))
    march(ctx, army, army[0].tile, (360, 60))

    assert {args[0] for args in client.named("move")} == {u.id for u in army}
    assert ctx._land_queries <= 8


def test_a_hit_on_a_worker_recalls_the_army_even_when_the_attacker_is_out_of_view():
    soldiers = [unit(300 + i, UnitKind.SPEARMAN, tile=(245, 220)) for i in range(12)]
    world, hit = raid(soldiers, raider_visible=False)
    hit.id, hit.is_building = 1, False
    client = FakeClient()
    ctx = Context(client, decision(world, [hit]), Blackboard(), Config())
    CombatManager().step(ctx)

    (members, *_), = client.named("formation_move")
    assert members == [u.id for u in soldiers]
    assert ctx.board.counter_owner is None


def test_a_field_force_can_march_to_a_goal_outside_its_terrain_view():
    from aoeworld import Visibility

    army = [unit(300 + i, UnitKind.SPEARMAN, tile=(145, 150)) for i in range(4)]
    workers, buildings = base(3)
    world = FakeWorld(workers + army, buildings)
    world.land_known = lambda *_: False
    world.visibility = lambda *_: Visibility.UNEXPLORED
    world.land_route = lambda *_: None
    client = FakeClient()
    ctx = Context(client, decision(world), Blackboard(), Config())

    march(ctx, army, (145, 150), (215, 215))

    assert [(members, x, y) for members, x, y, _ in client.named("formation_move")] == [
        ([soldier.id for soldier in army], 215, 215)]

"""Arena diagnostics from the same fog-filtered observations as the player."""

from collections import Counter
import json
import os
from pathlib import Path
import time

from aoeworld import CommandResult

RUN_ID = f"{time.time_ns()}-{os.getpid()}"


def emit(payload):
    directory = Path(".cache/ai-arena")
    directory.mkdir(parents=True, exist_ok=True)
    path = directory / f"seed-{payload['seed']}-{RUN_ID}.jsonl"
    with path.open("a") as stream:
        stream.write(json.dumps(payload) + "\n")


def outcomes(host, index, decision, seed):
    if decision is None:
        return
    board = host.brains[index].board
    for event in decision.events:
        if event.kind == "order_result" and event.result != CommandResult.ACCEPTED:
            tracked = board.tracked.get(event.sequence)
            if tracked is None:
                continue
            key = f"rejected:{tracked.name}:{event.result}".replace("CommandResult.", "")
            if board.stats.get(key, 0) < 3:
                emit({
                    "seed": seed, "bot": index, "tick": decision.tick,
                    "order": tracked.name, "args": str(tracked.args),
                    "result": str(event.result),
                })
        elif event.kind == "under_attack":
            emit({
                "seed": seed, "bot": index, "tick": decision.tick,
                "victim": event.id, "attacker": event.attacker,
            })


def trace(host, tick, seed):
    for index, client in enumerate(host.clients):
        world, board = client.world, host.brains[index].board
        units = world.my_units()
        eco = world.economy
        payload = {
            "seed": seed, "minute": round(tick / 1200, 2), "bot": index,
            "stock": [eco.food, eco.wood, eco.gold, eco.stone],
            "tasks": dict(Counter(f"{u.kind}:{u.task}" for u in units)),
            "stats": board.stats,
            "target": board.attack_target,
            "age": str(eco.age),
            "buildings": [{"id": b.id, "kind": str(b.kind), "tile": b.center,
                           "complete": b.complete, "progress": b.progress}
                          for b in world.my_buildings()],
            "units": [{"id": u.id, "kind": str(u.kind), "tile": u.tile, "hp": u.hp,
                       "task": str(u.task), "moving": u.moving,
                       "target": u.attack_target, "gathering": str(u.gathering),
                       "carrying": str(u.carrying)} for u in units],
            "enemy_buildings": [{"id": b.id, "tile": b.center,
                                 "visible": b.visible, "hp": b.hp,
                                 "ground": world.passable(*b.center)}
                                for b in world.known_enemy_buildings()],
            "enemies": [{"id": u.id, "tile": u.tile, "hp": u.hp}
                        for u in world.visible_enemies()],
            "animals": [{"id": a.id, "kind": str(a.kind), "tile": a.tile,
                         "hp": a.hp, "food": a.food_remaining,
                         "task": str(a.task)} for a in world.animals()],
        }
        emit(payload)

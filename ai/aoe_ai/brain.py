"""The player: one :class:`Brain` per slot, called at every decision point."""

from __future__ import annotations

from typing import Any, Optional

import aoeworld
from aoeworld import CommandResult

from .combat import CombatManager
from .config import Config
from .constructor import Constructor
from .context import Context
from .economy import EconomyManager
from .memory import BotMemory
from .military import MilitaryManager
from .research import ResearchManager
from .scouting import ScoutManager

#: Refusals that mean "this target will never work for this unit".
HOPELESS = (
    CommandResult.REJECTED_INVALID_RESOURCE,
    CommandResult.REJECTED_WRONG_UNIT_KIND,
    CommandResult.REJECTED_UNREACHABLE,
    CommandResult.REJECTED_RESOURCE_UNAVAILABLE,
    CommandResult.REJECTED_UNKNOWN_ENTITY,
)


class Brain:
    """Runs the managers in priority order and learns from refusals.

    Use it as the ``on_tick`` of :meth:`aoeworld.Client.run`: ``brain.on_tick``.
    """

    def __init__(self, config: Optional[Config] = None, name: str = "bot") -> None:
        self.config = config or Config()
        self.name = name
        self.memory = BotMemory()
        self.constructor = Constructor()
        self.research = ResearchManager()
        self.economy = EconomyManager(self.constructor)
        self.military = MilitaryManager()
        self.scouts = ScoutManager()
        self.combat = CombatManager()

    def on_tick(self, client: Any, decision: Any) -> None:
        self.decide(client, decision)

    def decide(self, client: Any, decision: Any) -> None:
        ctx = Context(client, decision, self.memory, self.config)
        self.absorb(ctx)
        if ctx.economy is None:
            return
        # Priority order = spending order.
        self.constructor.refresh(ctx)
        self.research.step(ctx)
        self.combat.step(ctx)
        self.economy.produce(ctx)
        self.constructor.houses(ctx)
        self.economy.work(ctx)
        self.constructor.step(ctx)
        self.military.step(ctx)
        self.scouts.step(ctx)
        self.memory.bump("decisions")

    # ----- learning from the server's verdicts -----------------------------
    def absorb(self, ctx: Context) -> None:
        mem = ctx.memory
        for event in ctx.events:
            if event.kind == "order_result":
                tracked = mem.tracked.pop(event.sequence, None)
                if event.result == CommandResult.ACCEPTED:
                    mem.bump("accepted")
                else:
                    mem.bump("rejected")
                    mem.bump("rejected:" + str(event.result))
                    if tracked is not None:
                        self.on_rejected(ctx, tracked, event.result)
            elif event.kind == "unit_died" and event.mine:
                mem.bump("units_lost")
            elif event.kind == "unit_died":
                mem.bump("kills")
            elif event.kind == "building_completed":
                mem.bump("buildings_completed")
            elif event.kind == "age_advanced":
                mem.bump("ages")
                mem.blocked_units.clear()

    def on_rejected(self, ctx: Context, tracked: Any, result: Any) -> None:
        mem = ctx.memory
        name, args = tracked.name, tracked.args
        if name == "gather" and result in HOPELESS:
            mem.bad_resources.add(args[1])
        elif name == "build":
            kind, x, y = args[1], args[2], args[3]
            mem.bad_sites.add((str(kind), x, y))
            mem.plans = [p for p in mem.plans if p.tile != (x, y)]
            for builder in args[0]:
                mem.builders.pop(builder, None)
        elif name == "farm":
            mem.farmers.pop(args[1], None)
        elif name == "train" and result == CommandResult.REJECTED_REQUIRES_FEUDAL_AGE:
            mem.blocked_units.add(str(args[1]))
        elif name == "research":
            mem.cool("research:" + str(args[1]), ctx.tick, 8 * ctx.world.tick_hz)

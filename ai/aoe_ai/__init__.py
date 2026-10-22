"""A complete Dark and Feudal Age player for AoeWorld.

The package is organised like the player it implements: a :class:`Brain`
runs one manager per concern each decision (economy, construction, research,
military, scouting, combat), all reading the same fog-filtered ``World`` and
sharing one :class:`BotMemory`.  :mod:`aoe_ai.host` runs many brains in one
process.
"""

from .brain import Brain
from .config import Config
from .memory import BotMemory

__all__ = ["Brain", "BotMemory", "Config"]

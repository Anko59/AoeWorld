"""Every tunable of the player in one place."""

from __future__ import annotations

from dataclasses import dataclass, field


@dataclass
class Config:
    """Tunables. The defaults play a standard boom into a Feudal army."""

    # --- decision pacing ---------------------------------------------------
    decision_hz: float = 2.0

    # --- economy -----------------------------------------------------------
    #: Stop training villagers at this count.
    max_villagers: int = 60
    #: Villagers the Town Center queue is kept at.
    town_center_queue: int = 2
    #: Build a house when fewer than this many population slots are free.
    house_margin: int = 4
    #: Preferred share of villagers per resource before and after Feudal.
    dark_shares: dict[str, float] = field(
        default_factory=lambda: {"FOOD": 0.50, "WOOD": 0.38, "GOLD": 0.06, "STONE": 0.06}
    )
    feudal_shares: dict[str, float] = field(
        default_factory=lambda: {"FOOD": 0.38, "WOOD": 0.34, "GOLD": 0.20, "STONE": 0.08}
    )
    #: A resource node farther than this from every drop-off needs a camp.
    dropoff_distance: int = 9
    #: Never send a villager to a node farther than this from the Town Center.
    max_gather_distance: int = 70
    #: Hunting is considered while an animal is this close to the Town Center.
    hunt_distance: int = 30
    #: Farms wanted per food-assigned villager once hunting and foraging end.
    farm_target: int = 6

    # --- construction ------------------------------------------------------
    #: Wood kept back for houses when spending on anything else.
    wood_reserve: int = 50
    #: Seconds before an unconfirmed planned building is forgotten.
    plan_timeout_seconds: int = 60
    build_search_radius: int = 18

    # --- age ---------------------------------------------------------------
    #: Research Loom once this many villagers work.
    loom_villagers: int = 10
    #: Save for and click Feudal Age at this many villagers (DE players go up
    #: at 20-22) once two kinds of the required buildings stand.
    feudal_villagers: int = 21

    # --- military ----------------------------------------------------------
    #: The army is trained from Feudal Age on, never before: Dark Age food
    #: belongs to villagers and the age.
    army_min_villagers: int = 18
    #: Spearmen : archers : skirmishers.
    army_mix: dict[str, float] = field(
        default_factory=lambda: {"SPEARMAN": 4.0, "ARCHER": 3.0, "SKIRMISHER": 3.0}
    )
    max_barracks: int = 2
    max_ranges: int = 2

    # --- scouting ----------------------------------------------------------
    scout_ring_step: int = 13
    scout_max_radius: int = 120

    # --- combat ------------------------------------------------------------
    #: Army size that starts an attack.
    attack_army_size: int = 12
    #: Retreat when the army drops below this fraction of its launch size.
    retreat_fraction: float = 0.4
    #: Defend when a threat is this close to the base.
    defend_radius: int = 24
    rally_offset: int = 7

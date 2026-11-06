"""Every tunable of the player in one place."""

from __future__ import annotations

from dataclasses import dataclass, field


@dataclass
class Config:
    """Tunables. The defaults play a standard boom into a Feudal army."""

    #: Which draft this is (see `aoe_ai/drafts`); shown in tournament reports.
    name: str = "baseline"

    # --- decision pacing ---------------------------------------------------
    decision_hz: float = 2.0
    #: Enable the bounded known-water crossing heuristic for army targets.
    build_bridges: bool = False
    bridge_retry_seconds: int = 30

    # --- economy -----------------------------------------------------------
    #: Stop training villagers at this count.
    max_villagers: int = 60
    #: Save 800 food and 200 gold after the Feudal production buildings stand.
    castle_villagers: int = 28
    castle_army_min: int = 12
    castle_knight_weight: float = 1.0
    #: Save 1000 food and 800 gold for Imperial Age with this many villagers,
    #: a standing Castle Age army and two Castle Age buildings.
    imperial_villagers: int = 40
    imperial_army_min: int = 24
    #: Villagers the Town Center queue is kept at.
    town_center_queue: int = 2
    #: Build a house when fewer than this many population slots are free.
    house_margin: int = 4
    #: Houses under construction at once (plus one per extra Town Center).
    house_pending: int = 2
    #: Preferred share of villagers per resource before and after Feudal.
    dark_shares: dict[str, float] = field(
        default_factory=lambda: {"FOOD": 0.60, "WOOD": 0.32, "GOLD": 0.08, "STONE": 0.0}
    )
    feudal_shares: dict[str, float] = field(
        default_factory=lambda: {"FOOD": 0.46, "WOOD": 0.30, "GOLD": 0.24, "STONE": 0.0}
    )
    #: Gold-heavy shares while the Imperial Age is being saved for.
    imperial_shares: dict[str, float] = field(
        default_factory=lambda: {"FOOD": 0.40, "WOOD": 0.25, "GOLD": 0.35, "STONE": 0.0}
    )
    #: A resource node farther than this from every drop-off needs a camp.
    dropoff_distance: int = 9
    #: A proactive camp uses a tree beside its woodcutter, without routing.
    wood_camp_contact_tiles: int = 2
    #: Never send a villager to a node farther than this from the Town Center.
    max_gather_distance: int = 70
    #: Hunting is considered while an animal is this close to the Town Center.
    hunt_distance: int = 30
    #: Farms wanted per food-assigned villager once hunting and foraging end.
    farm_target: int = 6
    #: Prepaid farm rebuilds retained at each Mill once farms exist.
    farm_reseed_queue: int = 4
    reseed_settle_seconds: int = 2
    #: A refused farm may be occupied by an automatic farmer still off screen.
    farm_refusal_wait_seconds: int = 30

    # --- construction ------------------------------------------------------
    #: Wood kept back for houses when spending on anything else.
    wood_reserve: int = 50
    #: Seconds before an unconfirmed planned building is forgotten.
    plan_timeout_seconds: int = 60
    build_search_radius: int = 18
    #: Leave two-way worker lanes between the core village buildings.
    building_gap: int = 2
    #: Seconds an order kind waits after the server reports a world limit.
    world_limit_wait_seconds: int = 30

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

    # --- optional river fleet ----------------------------------------------
    fishing_ships: int = 1
    galleys: int = 2
    dock_search_radius: int = 12
    naval_settle_seconds: int = 2

    # --- scouting ----------------------------------------------------------
    scout_ring_step: int = 13
    scout_max_radius: int = 120
    #: A scout idle this far from its waypoint did not get there (no route):
    #: queued waypoints at least as far away are dropped. Each such order
    #: costs the server a long hopeless route search.
    scout_unreachable_tiles: int = 100
    scout_settle_seconds: int = 2
    #: A villager idle again this soon after a gather order never got to the
    #: node (no route: across a river, inside a forest). The node is left
    #: alone for `gather_unreachable_seconds` and another one is chosen.
    #: An army marches to a far target in hops of this many tiles (a short
    #: route each, planned at once); one order across the whole map per
    #: soldier made the server plan thousands of map-wide routes at a time.
    march_hop_tiles: int = 48
    march_group_tiles: int = 16
    army_formation: str = "LINE"
    #: A soldier is sent on at most once per this many seconds, and an army
    #: that got no nearer its goal for `march_stall_seconds` gives it up.
    march_pause_seconds: int = 5
    march_stall_seconds: int = 60
    gather_fail_seconds: int = 20
    gather_unreachable_seconds: int = 180

    #: Bound the public land queries across every manager in one decision.
    land_queries_per_decision: int = 16
    land_retry_seconds: int = 10
    land_cache_entries: int = 1024

    # --- combat ------------------------------------------------------------
    #: Army size that starts an attack.
    attack_army_size: int = 12
    #: Retreat when the army drops below this fraction of its launch size.
    retreat_fraction: float = 0.4
    #: Defend when a threat is this close to the base.
    defend_radius: int = 24
    rally_offset: int = 7
    #: A wound counts as ours when a soldier of ours stood within this many tiles.
    hit_reach: int = 12
    #: Retry refused attack targets after the battlefield has had time to change.
    attack_refusal_wait_seconds: int = 30
    #: Protect working villagers from nearby wolves and provoked boars.
    predator_worker_radius: int = 6
    predator_defender_radius: int = 12
    predator_soldier_group: int = 2
    predator_worker_group: int = 4

    #: Stop army spending while the Castle Age price is saved for.
    castle_first: bool = False

    # --- management units (docs/economy/steward.md, docs/gameplay/general.md) --
    #: Castles to raise (650 stone each) once in Castle Age; Vassals and
    #: Generals are trained there.
    castles: int = 0
    #: Stone share of the villagers while a wanted Castle is unpaid.
    castle_stone_share: float = 0.15
    #: Town Centers to raise (the first is the start's) and the villagers
    #: from which the next is placed; each trains villagers.
    town_centers: int = 1
    expand_villagers: int = 36
    expand_distance: int = 28
    #: Stewards to field (one per Town Center), from this many villagers.
    stewards: int = 0
    steward_min_villagers: int = 30
    #: Leave villagers a Steward manages to the Steward (no gather orders).
    defer_to_steward: bool = False
    #: Vassals to field (one per Castle) and Generals in total.
    vassals: int = 0
    generals: int = 0
    #: Save the next management unit's price ahead of army spending.
    save_for_management: bool = True
    save_seconds: int = 240
    save_pause_seconds: int = 180
    #: General unit-mix sliders: infantry, archers, heavy cavalry, siege, scouts.
    general_mix: tuple = (35, 30, 20, 10, 5)
    general_train: bool = True
    #: A General marches once he leads this many soldiers, and falls back
    #: to the Castle when he leads fewer than `general_retreat_army`.
    general_attack_army: int = 30
    general_retreat_army: int = 10


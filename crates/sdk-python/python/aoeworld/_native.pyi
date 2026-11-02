"""Type stubs of the native module (kept in sync by hand; see tests/test_stubs.py)."""

from typing import Callable, Optional, Sequence, Tuple

class CombatStance:
    AGGRESSIVE: CombatStance
    DEFENSIVE: CombatStance
    STAND_GROUND: CombatStance
    NO_ATTACK: CombatStance
    def __int__(self) -> int: ...
    def __hash__(self) -> int: ...

class Formation:
    LINE: Formation
    BOX: Formation
    STAGGERED: Formation
    FLANK: Formation
    def __int__(self) -> int: ...
    def __hash__(self) -> int: ...

class GeneralState:
    IDLE: GeneralState
    MARCHING: GeneralState
    FIGHTING: GeneralState
    RETREATING: GeneralState

class UnitKind:
    VILLAGER: UnitKind
    SCOUT: UnitKind
    MILITIA: UnitKind
    FISHING_SHIP: UnitKind
    ARCHER: UnitKind
    SKIRMISHER: UnitKind
    SPEARMAN: UnitKind
    MAN_AT_ARMS: UnitKind
    LONG_SWORDSMAN: UnitKind
    PIKEMAN: UnitKind
    CROSSBOWMAN: UnitKind
    ELITE_SKIRMISHER: UnitKind
    KNIGHT: UnitKind
    LIGHT_CAVALRY: UnitKind
    MONK: UnitKind
    MANGONEL: UnitKind
    BATTERING_RAM: UnitKind
    GALLEY: UnitKind
    FIRE_SHIP: UnitKind
    DEMOLITION_SHIP: UnitKind
    TRANSPORT_SHIP: UnitKind
    WAR_GALLEY: UnitKind
    VASSAL: UnitKind
    GENERAL: UnitKind
    def __int__(self) -> int: ...
    def __hash__(self) -> int: ...

class BuildingKind:
    TOWN_CENTER: BuildingKind
    HOUSE: BuildingKind
    LUMBER_CAMP: BuildingKind
    MINING_CAMP: BuildingKind
    MILL: BuildingKind
    FARM: BuildingKind
    BARRACKS: BuildingKind
    PALISADE_WALL: BuildingKind
    OUTPOST: BuildingKind
    DOCK: BuildingKind
    ARCHERY_RANGE: BuildingKind
    STABLE: BuildingKind
    BLACKSMITH: BuildingKind
    MARKET: BuildingKind
    WATCH_TOWER: BuildingKind
    CASTLE: BuildingKind
    BRIDGE: BuildingKind
    PALISADE_GATE: BuildingKind
    CASTLE: BuildingKind
    MONASTERY: BuildingKind
    UNIVERSITY: BuildingKind
    SIEGE_WORKSHOP: BuildingKind
    STONE_WALL: BuildingKind
    STONE_GATE: BuildingKind
    def __int__(self) -> int: ...
    def __hash__(self) -> int: ...

class ResourceKind:
    FOOD: ResourceKind
    WOOD: ResourceKind
    GOLD: ResourceKind
    STONE: ResourceKind
    def __int__(self) -> int: ...
    def __hash__(self) -> int: ...

class TradeResource:
    WOOD: TradeResource
    FOOD: TradeResource
    STONE: TradeResource
    def __int__(self) -> int: ...
    def __hash__(self) -> int: ...

class Tech:
    LOOM: Tech
    TOWN_WATCH: Tech
    FEUDAL_AGE: Tech
    DOUBLE_BIT_AXE: Tech
    HORSE_COLLAR: Tech
    GOLD_MINING: Tech
    STONE_MINING: Tech
    WHEELBARROW: Tech
    MAN_AT_ARMS: Tech
    FORGING: Tech
    SCALE_MAIL_ARMOR: Tech
    SCALE_BARDING_ARMOR: Tech
    FLETCHING: Tech
    PADDED_ARCHER_ARMOR: Tech
    CASTLE_AGE: Tech
    LONG_SWORDSMAN: Tech
    PIKEMAN: Tech
    CROSSBOWMAN: Tech
    ELITE_SKIRMISHER: Tech
    LIGHT_CAVALRY: Tech
    HAND_CART: Tech
    TOWN_PATROL: Tech
    IRON_CASTING: Tech
    CHAIN_MAIL_ARMOR: Tech
    CHAIN_BARDING_ARMOR: Tech
    BODKIN_ARROW: Tech
    LEATHER_ARCHER_ARMOR: Tech
    HEAVY_PLOW: Tech
    BOW_SAW: Tech
    GOLD_SHAFT_MINING: Tech
    MASONRY: Tech
    BALLISTICS: Tech
    MURDER_HOLES: Tech
    WAR_GALLEY: Tech
    def __int__(self) -> int: ...
    def __hash__(self) -> int: ...

class Age:
    DARK: Age
    FEUDAL: Age
    CASTLE: Age
    def __int__(self) -> int: ...
    def __hash__(self) -> int: ...

class UnitTask:
    IDLE: UnitTask
    MOVING: UnitTask
    GATHERING: UnitTask
    CARRYING: UnitTask
    RETURNING: UnitTask
    ATTACKING: UnitTask
    DYING: UnitTask
    BUILDING: UnitTask
    FARMING: UnitTask
    HUNTING: UnitTask
    SHEPHERDING: UnitTask
    FISHING: UnitTask
    def __int__(self) -> int: ...
    def __hash__(self) -> int: ...

class AnimalKind:
    SHEEP: AnimalKind
    DEER: AnimalKind
    BOAR: AnimalKind
    WOLF: AnimalKind
    def __int__(self) -> int: ...
    def __hash__(self) -> int: ...

class AnimalTask:
    IDLE: AnimalTask
    FLEEING: AnimalTask
    ATTACKING: AnimalTask
    DEAD: AnimalTask
    CARCASS: AnimalTask
    def __int__(self) -> int: ...
    def __hash__(self) -> int: ...

class Visibility:
    UNEXPLORED: Visibility
    EXPLORED: Visibility
    VISIBLE: Visibility
    def __int__(self) -> int: ...
    def __hash__(self) -> int: ...

class CommandResult:
    ACCEPTED: CommandResult
    REJECTED_DISCONNECTED: CommandResult
    REJECTED_UNKNOWN_ENTITY: CommandResult
    REJECTED_SEQUENCE: CommandResult
    REJECTED_RATE_LIMITED: CommandResult
    REJECTED_QUEUE_FULL: CommandResult
    REJECTED_INVALID_DESTINATION: CommandResult
    REJECTED_UNREACHABLE: CommandResult
    REJECTED_PATH_BUDGET_EXCEEDED: CommandResult
    REJECTED_NOT_OWNER: CommandResult
    REJECTED_WRONG_UNIT_KIND: CommandResult
    REJECTED_INVALID_RESOURCE: CommandResult
    REJECTED_INSUFFICIENT_FOOD: CommandResult
    REJECTED_TRAINING_QUEUE_FULL: CommandResult
    REJECTED_RESOURCE_UNAVAILABLE: CommandResult
    REJECTED_INVALID_TARGET: CommandResult
    REJECTED_INSUFFICIENT_RESOURCES: CommandResult
    REJECTED_NOT_BUILDABLE: CommandResult
    REJECTED_REQUIREMENT_MISSING: CommandResult
    REJECTED_INVALID_PLACEMENT: CommandResult
    REJECTED_UNDER_CONSTRUCTION: CommandResult
    REJECTED_ALREADY_COMPLETE: CommandResult
    REJECTED_FARM_UNAVAILABLE: CommandResult
    REJECTED_NOT_RESEARCHABLE: CommandResult
    REJECTED_ALREADY_RESEARCHED: CommandResult
    REJECTED_RESEARCH_BUSY: CommandResult
    REJECTED_TECH_REQUIREMENT: CommandResult
    REJECTED_REQUIRES_FEUDAL_AGE: CommandResult
    REJECTED_NO_MARKET: CommandResult
    REJECTED_WORLD_LIMIT: CommandResult
    REJECTED_ORDER_TOO_LARGE: CommandResult
    REJECTED_GARRISON_FULL: CommandResult
    REJECTED_VASSAL_EXISTS: CommandResult
    REJECTED_VASSAL_LEASH: CommandResult
    REJECTED_VASSAL_HOME: CommandResult
    REJECTED_UNEXPLORED_PLACEMENT: CommandResult
    REJECTED_REQUIRES_CASTLE_AGE: CommandResult
    def __int__(self) -> int: ...
    def __hash__(self) -> int: ...


class AoeError(Exception): ...
class ConnectionClosed(AoeError): ...
class Refused(AoeError): ...

class Unit:
    id: int
    owner: int
    mine: bool
    kind: UnitKind
    x: int
    y: int
    task: UnitTask
    hp: int
    max_hp: int
    moving: bool
    auto_scout: bool
    cargo_count: int
    home_castle: Optional[int]
    managed_by: Optional[int]
    commanded_by: Optional[int]
    army_size: Optional[int]
    general_state: Optional[GeneralState]
    idle: bool
    alive: bool
    military: bool
    carrying: Optional[tuple[ResourceKind, int]]
    gathering: Optional[ResourceKind]
    attack_target: Optional[int]
    stance: CombatStance
    @property
    def tile(self) -> tuple[int, int]: ...
    @property
    def health(self) -> float: ...

class Building:
    id: int
    owner: int
    mine: bool
    kind: BuildingKind
    x: int
    y: int
    width: int
    height: int
    complete: bool
    progress: int
    hp: int
    max_hp: int
    visible: bool
    queue_length: int
    farm_reseeds: int
    garrison_capacity: int
    garrison: list[tuple[int, UnitKind, int, int]]
    rally_position: Optional[tuple[int, int]]
    rally_resource: Optional[int]
    rally_building: Optional[int]
    rally_animal: Optional[int]
    researching: Optional[Tech]
    idle: bool
    @property
    def center(self) -> tuple[int, int]: ...

class ResourceNode:
    id: int
    x: int
    y: int
    kind: ResourceKind
    remaining: int
    @property
    def tile(self) -> tuple[int, int]: ...

class Animal:
    id: int
    kind: AnimalKind
    x: int
    y: int
    hp: int
    task: AnimalTask
    food_remaining: int
    owner: Optional[int]
    @property
    def tile(self) -> tuple[int, int]: ...

class Economy:
    food: int
    wood: int
    gold: int
    stone: int
    population: int
    age: Age
    researched: list[Tech]
    wood_price: int
    food_price: int
    stone_price: int
    def has_researched(self, tech: Tech) -> bool: ...
    def can_afford(self, food: int = 0, wood: int = 0, gold: int = 0, stone: int = 0) -> bool: ...

class Event:
    kind: str
    id: Optional[int]
    unit_kind: Optional[UnitKind]
    building_kind: Optional[BuildingKind]
    owner: Optional[int]
    mine: Optional[bool]
    tech: Optional[Tech]
    age: Optional[Age]
    x: Optional[int]
    y: Optional[int]
    attacker: Optional[int]
    is_building: Optional[bool]
    sequence: Optional[int]
    result: Optional[CommandResult]
    code: Optional[int]
    message: Optional[str]

class World:
    def move_queue(self, unit: int) -> list[tuple[int, int]]: ...
    @property
    def tick(self) -> int: ...
    @property
    def tick_hz(self) -> int: ...
    @property
    def seconds(self) -> int: ...
    @property
    def map_size(self) -> tuple[int, int]: ...
    @property
    def me(self) -> Optional[int]: ...
    @property
    def economy(self) -> Optional[Economy]: ...
    def my_units(self, kind: Optional[UnitKind] = None) -> list[Unit]: ...
    def idle_units(self, kind: Optional[UnitKind] = None) -> list[Unit]: ...
    def visible_enemies(self) -> list[Unit]: ...
    def unit(self, id: int) -> Optional[Unit]: ...
    def my_buildings(self, kind: Optional[BuildingKind] = None, complete_only: bool = False) -> list[Building]: ...
    def known_enemy_buildings(self) -> list[Building]: ...
    def building(self, id: int) -> Optional[Building]: ...
    def town_center(self) -> Optional[Building]: ...
    def unfinished_buildings(self) -> list[Building]: ...
    def animals(self) -> list[Animal]: ...
    def resources(self, kind: Optional[ResourceKind] = None) -> list[ResourceNode]: ...
    def nearest_resource(self, kind: ResourceKind, x: int, y: int) -> Optional[ResourceNode]: ...
    def visibility(self, x: int, y: int) -> Visibility: ...
    @property
    def loaded_chunks(self) -> int: ...
    def find_bridge_site(self, x: int, y: int, tx: int, ty: int) -> Optional[tuple[tuple[int, int], tuple[int, int]]]: ...
    def passable(self, x: int, y: int) -> Optional[bool]: ...
    def land_route(self, x: int, y: int, tx: int, ty: int) -> Optional[list[tuple[int, int]]]: ...
    def land_frontier(self, x: int, y: int, tx: int, ty: int) -> Optional[tuple[int, int]]: ...
    def find_build_site(self, kind: BuildingKind, x: int, y: int, max_radius: int = 24, gap: int = 1) -> Optional[tuple[int, int]]: ...

class Decision:
    world: World
    events: list[Event]
    tick: int

class RunSummary:
    decisions: int
    last_tick: int
    closed: bool
    close_reason: Optional[str]

class Client:
    def wait_decision(self, timeout: Optional[float] = None) -> Optional[Decision]: ...
    @property
    def world(self) -> World: ...
    @property
    def orders_sent(self) -> int: ...
    @property
    def closed(self) -> bool: ...
    @property
    def close_reason(self) -> Optional[str]: ...
    @property
    def connection_stats(self) -> Tuple[int, int, float]: ...
    def run(
        self,
        on_tick: Callable[["Client", Decision], Optional[bool]],
        until_tick: Optional[int] = None,
        max_decisions: Optional[int] = None,
    ) -> RunSummary: ...
    def close(self) -> None: ...
    def move(self, unit: int, x: int, y: int) -> int: ...
    def stop(self, unit: int) -> int: ...
    def load(self, unit_ids: Sequence[int], transport_id: int) -> int: ...
    def unload(self, transport_id: int, x: int, y: int) -> int: ...
    def set_rally(self, building: int, x: int, y: int) -> int: ...
    def set_formation(self, units: Sequence[int], formation: Formation) -> int: ...
    def set_stance(self, units: Sequence[int], stance: CombatStance) -> int: ...
    def formation_move(self, units: Sequence[int], x: int, y: int, formation: Formation = ..., *, queued: bool = False, attack_move: bool = False) -> int: ...
    def queue_move(self, unit: int, x: int, y: int) -> int: ...
    def set_rally_resource(self, building: int, resource: int) -> int: ...
    def set_rally_building(self, building: int, target: int) -> int: ...
    def set_rally_animal(self, building: int, animal: int) -> int: ...
    def gather(self, unit: int, resource: int) -> int: ...
    def build(self, builders: Sequence[int], kind: BuildingKind, x: int, y: int) -> int: ...
    def build_bridge(self, builders: Sequence[int], x1: int, y1: int, x2: int, y2: int) -> int: ...
    def build_wall(self, builders: Sequence[int], x1: int, y1: int, x2: int, y2: int, kind: BuildingKind = ...) -> int: ...
    def construct(self, builders: Sequence[int], building: int) -> int: ...
    def cancel_construction(self, building: int) -> int: ...
    def garrison(self, unit: int, building: int) -> int: ...
    def ungarrison(self, building: int, unit: Optional[int] = None) -> int: ...
    def auto_scout(self, unit: int, enabled: bool = True) -> int: ...
    def farm(self, unit: int, farm: int) -> int: ...
    def farm_reseed(self, building: int, add: bool = True) -> int: ...
    def train(self, building: int, kind: UnitKind) -> int: ...
    def cancel_training(self, building: int) -> int: ...
    def research(self, building: int, tech: Tech) -> int: ...
    def cancel_research(self, building: int) -> int: ...
    def attack(self, unit: int, target: int) -> int: ...
    def hunt(self, units: Sequence[int], animal: int) -> int: ...
    def market_buy(self, building: int, resource: TradeResource) -> int: ...
    def market_sell(self, building: int, resource: TradeResource) -> int: ...
    def __enter__(self) -> "Client": ...
    def __exit__(self, *args: object) -> None: ...

class Fleet:
    def __init__(self, clients: Sequence[Client]) -> None: ...
    def __len__(self) -> int: ...
    def wait(self, timeout: Optional[float] = None) -> list[tuple[int, Optional[Decision]]]: ...

class SlotAssignment:
    slot: int
    player: int
    kind: str
    token: str
    primary_unit_id: int
    start_tile: tuple[int, int]

class MatchInfo:
    world_id: int
    content_hash: str
    tiles_per_side: int
    slots: list[SlotAssignment]

def connect(
    url: str,
    slot_token: str,
    decision_hz: float = 2.0,
    view_padding: int = 28,
    view_size: int = 128,
    fetch_terrain: bool = True,
    orders_per_second: int = 90,
) -> Client: ...
def start_match(
    url: str,
    admin_token: str,
    slots: Sequence[str],
    tiles_per_side: int = 500,
    seed: int = 1,
    terrain: str = "flat",
    map_hash: Optional[str] = None,
) -> MatchInfo: ...

Cost = tuple[int, int, int, int]

def building_cost(kind: BuildingKind) -> Cost: ...
def building_buildable(kind: BuildingKind) -> bool: ...
def building_required_age(kind: BuildingKind) -> Age: ...
def build_seconds(kind: BuildingKind) -> int: ...
def building_size(kind: BuildingKind) -> tuple[int, int]: ...
def building_requires(kind: BuildingKind) -> Optional[BuildingKind]: ...
def trainable(building: BuildingKind) -> list[UnitKind]: ...
def unit_cost(kind: UnitKind) -> Cost: ...
def train_seconds(kind: UnitKind) -> int: ...
def tech_cost(tech: Tech) -> Cost: ...
def villager_capacity() -> int: ...

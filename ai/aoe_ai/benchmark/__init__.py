"""Measurements of the ordinary Brain, without privileged decision inputs."""
from aoeworld import BuildingKind, UnitKind

SETTLEMENTS = (BuildingKind.TOWN_CENTER, BuildingKind.CASTLE, BuildingKind.BARRACKS,
               BuildingKind.ARCHERY_RANGE, BuildingKind.STABLE, BuildingKind.SIEGE_WORKSHOP)


class Checkpoints:
    """Samples retain actual ticks; late observations cannot fill missed history."""
    def __init__(self, minutes=(30, 60, 120), tick_hz=20):
        self.minutes = list(minutes)
        self.tick_hz = tick_hz
        self.samples = []

    @property
    def complete(self):
        return len(self.samples) == len(self.minutes)

    def due(self, tick):
        if self.complete:
            return None
        minute = self.minutes[len(self.samples)]
        target = minute * 60 * self.tick_hz
        if tick > target + self.tick_hz:
            raise ValueError(f'missed {minute}-minute checkpoint at tick {tick}')
        return minute if tick >= target else None

    def record(self, tick, players):
        if not players:
            raise ValueError('empty checkpoint')
        minute = self.due(tick)
        if minute is None:
            raise ValueError('checkpoint is not due')
        self.samples.append(dict(minute=minute, tick=tick, players=players))


def measure(world, *, kills, orders):
    eco = world.economy
    if eco is None:
        raise ValueError('missing player economy')
    buildings = [b for b in world.my_buildings() if b.complete]
    sites = [dict(id=b.id, kind=str(b.kind).split('.')[-1], tile=list(b.center))
             for b in buildings if b.kind in SETTLEMENTS]
    units = {u.id: u.kind for u in world.my_units()}
    units.update({entry[0]: entry[1] for b in buildings for entry in b.garrison})
    management = {name: sum(kind == target for kind in units.values()) for name, target in (
        ('stewards', UnitKind.STEWARD), ('vassals', UnitKind.VASSAL), ('generals', UnitKind.GENERAL))}
    fortifications = dict(castles=sum(b.kind == BuildingKind.CASTLE for b in buildings),
        towers=sum(b.kind == BuildingKind.WATCH_TOWER for b in buildings),
        wall_segments=sum(b.kind in (BuildingKind.PALISADE_WALL, BuildingKind.STONE_WALL,
            BuildingKind.PALISADE_GATE, BuildingKind.STONE_GATE) for b in buildings))
    return dict(management=management, fortifications=fortifications, player=world.me, observed_tick=world.tick, population=eco.population,
                town_centers=sum(b.kind == BuildingKind.TOWN_CENTER for b in buildings),
                settled_cells=len({(b.center[0] // 64, b.center[1] // 64)
                                   for b in buildings if b.kind in SETTLEMENTS}),
                settlement_sites=sites, age=str(eco.age).split('.')[-1],
                techs_researched=len(eco.researched),
                researched=[str(t).split('.')[-1] for t in eco.researched],
                resources={name: getattr(eco, name) for name in ('food', 'wood', 'gold', 'stone')},
                military=sum(u.military for u in world.my_units()), kills=kills, orders=orders)


class TickStream:
    """A fresh arena must provide every timing record, starting at tick one."""
    def __init__(self):
        self.records = []
        self.last_tick = 0

    def add(self, records, *, through_tick):
        expected = self.last_tick + 1
        for row in records:
            if row[0] != expected:
                raise ValueError(f'timing gap: expected tick {expected}, got {row[0]}')
            expected += 1
        if expected - 1 != through_tick:
            raise ValueError(f'missing timing records through tick {through_tick}')
        # The full phase maps are streamed to disk; retain compact columns
        # for run-wide statistics instead of a second in-memory phase history.
        self.records.extend(row[:8] for row in records)
        self.last_tick = through_tick

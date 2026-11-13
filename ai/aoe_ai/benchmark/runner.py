"""Lockstep observation, independent of the players' fog-filtered decisions."""
import hashlib
import json
import os
import time
import urllib.error
import urllib.request
from dataclasses import asdict
from pathlib import Path
from types import SimpleNamespace

import aoeworld

from .. import arena
from ..config import Config
from ..tournament import CostBrain
from . import Checkpoints, TickStream, measure


def request(url, path, body=None):
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(url + path, data=data, headers={
        'x-aoeworld-admin-token': arena.SECRET, 'content-type': 'application/json'})
    try:
        with urllib.request.urlopen(req, timeout=120) as response:
            return json.load(response)
    except urllib.error.HTTPError as error:
        raise RuntimeError(f'{path}: HTTP {error.code}: {error.read().decode()[:500]}') from error


def write(path, document):
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix('.tmp')
    tmp.write_text(json.dumps(document, indent=2) + '\n')
    tmp.replace(path)


class MeasuredBrain(CostBrain):
    def __init__(self, config, name):
        super().__init__(config, name)
        self.max_orders = 0
        self.wall_slowest = 0.0

    def decide(self, client, decision):
        before, started = client.orders_sent, time.monotonic()
        try:
            super().decide(client, decision)
        finally:
            self.max_orders = max(self.max_orders, client.orders_sent - before)
            self.wall_slowest = max(self.wall_slowest, time.monotonic() - started)


def viewer_files(root, setup):
    # Local, ignored bootstrap for filming one AI's ordinary fog-filtered view.
    # It sends no orders and is removed when this run exits.
    slot = setup['slots'][min(1, len(setup['slots']) - 1)]
    directory = root / 'web/pkg'
    directory.mkdir(parents=True, exist_ok=True)
    script, page = directory / 'ai-view.js', directory / 'ai-view.html'
    x, y = slot['start_tile']
    script.write_text("localStorage.setItem('aoeworld.resume-token', "
                      + json.dumps(slot['token']) + ");\n"
                      + f"location.replace('/#x={x}&y={y}&zoom=1.5');\n")
    page.write_text('<!doctype html><script src="/pkg/ai-view.js"></script>')
    return [script, page]


def source_hashes(root):
    paths = []
    for folder in ('ai/aoe_ai', 'crates/sdk/src', 'crates/sdk-python/src',
                   'crates/sdk-python/python/aoeworld', 'crates/server/src', 'crates/simulation/src', 'crates/map/src',
                   'crates/protocol/src', 'crates/harness/src/sdk_python'):
        paths.extend(p for p in (root / folder).rglob('*') if p.suffix in ('.py', '.rs', '.pyi'))
    paths.extend(root / p for p in ('Cargo.lock', 'Cargo.toml', 'rust-toolchain.toml',
                                    'crates/sdk-python/Cargo.lock', 'make/sdk.mk'))
    return {str(p.relative_to(root)): hashlib.sha256(p.read_bytes()).hexdigest()
            for p in sorted(paths)}


def run(args):
    root = Path(__file__).resolve().parents[3]
    if args.out.exists() or args.out.with_suffix('.ticks.jsonl').exists():
        raise ValueError('measurement output already exists; choose a fresh output path')
    workload = json.loads(args.starts.read_text())
    if (workload['map_hash'] != args.map_hash or workload['seed'] != args.seed
            or len(workload['slots']) != args.bots):
        raise ValueError('workload hash/seed/slot count must match command arguments')
    checkpoints = Checkpoints()
    brains = {}
    timings = TickStream()
    costs = timings.records
    cursor, seen = 0, -1
    identity = source_hashes(root)
    document = dict(source_files_sha256=identity, revision=os.environ['AOE_BENCHMARK_REVISION'], workload=workload,
                    config=asdict(Config()), complete=False, samples=checkpoints.samples)
    started = time.monotonic()
    # A fresh server for each workload keeps the timing cursor and world isolated.
    with arena.ArenaServer(args.server_bin, root) as server:
        setup = request(server.url, '/match', dict(map_hash=args.map_hash, seed=args.seed,
            slots=['ai'] * args.bots, start_tiles=[s['anchor'] for s in workload['slots']]))
        document['starts'] = [{k: v for k, v in s.items() if k != 'token'} for s in setup['slots']]
        write(args.out, document)
        for expected, actual in zip(workload['slots'], setup['slots']):
            if expected['kind'] == 'island' and actual['main_land'] is not False:
                raise ValueError(f"island start was not off the main land mass: {actual['start_tile']}")
            if expected['kind'] == 'forest' and actual['natural_trees_nearby'] < 200:
                raise ValueError(f"forest start has only {actual['natural_trees_nearby']} natural trees")
        # Do not retain credentials in the measurement artifact.
        info = SimpleNamespace(**setup)
        info.slots = [SimpleNamespace(**s) for s in setup['slots']]
        write(root / '.cache/ai-scaling/live.json', dict(url=server.url, pid=server.process.pid))
        if args.check_starts:
            document['setup_valid'] = True
            write(args.out, document)
            print(json.dumps(document['starts']), flush=True)
            return

        def observe(tick, host, _info):
            nonlocal cursor, seen
            if any(r.errors or r.closed or r.drops for r in host.reports):
                raise RuntimeError('bot error, closed connection or reconnect during benchmark')
            if tick // 1200 != seen:
                seen = tick // 1200
                page = request(server.url, f'/match/stats?since={cursor}')
                cursor = page['next']
                timings.add(page['ticks'], through_tick=tick)
                with args.out.with_suffix('.ticks.jsonl').open('a') as log:
                    for row in page['ticks']:
                        log.write(json.dumps(row) + '\n')
                print(f'game minute {seen}, tick {tick}', flush=True)
            if checkpoints.due(tick) is not None:
                admin = request(server.url, '/admin/match')
                kills = {p['id']: p['kills'] for p in admin['players']}
                players = []
                for i, client in enumerate(host.clients):
                    world = client.world
                    if abs(tick - world.tick) > world.tick_hz:
                        raise RuntimeError(f'stale slot {i} observation at checkpoint')
                    row = measure(world, kills=kills[world.me], orders=client.orders_sent)
                    row.update(slot=i, cpu_seconds=brains[i].cpu,
                        cpu_ms_per_decision=1000 * brains[i].cpu / max(1, brains[i].calls),
                        cpu_ms_slowest=1000 * brains[i].slowest,
                        wall_ms_slowest=1000 * brains[i].wall_slowest,
                        max_orders_per_decision=brains[i].max_orders)
                    players.append(row)
                checkpoints.record(tick, players)
                write(args.out, document)

        viewer = viewer_files(root, setup)
        try:
            result = arena.play(server.url, args.bots, 120, setup['tiles_per_side'], args.seed,
                match_info=info, observe=observe,
                brain_factory=lambda i: brains.setdefault(i, MeasuredBrain(Config(), f'bot{i}')))
            if source_hashes(root) != identity:
                raise RuntimeError('measured source changed during the run')
            document.update(result=result, complete=checkpoints.complete)
            if not document['complete']:
                raise RuntimeError('run ended before all checkpoints')
        except BaseException as error:
            document['failure'] = str(error)
            raise
        finally:
            document['wall_seconds'] = time.monotonic() - started
            totals = sorted(r[1] for r in costs)
            if totals:
                document['ticks'] = dict(count=len(totals),
                    tick_p50_us=totals[len(totals) // 2], tick_p95_us=totals[int(len(totals) * .95)],
                    tick_max_us=totals[-1], orders=sum(r[4] for r in costs))
            write(args.out, document)
            for path in viewer:
                path.unlink(missing_ok=True)
    print(json.dumps({k: document[k] for k in ('complete', 'wall_seconds', 'ticks')}), flush=True)

"""``python -m aoe_ai``: start a match with AI slots, or join one."""

from __future__ import annotations

import argparse
import json
import logging
import os
import sys
import time
from dataclasses import asdict

from .config import Config
from .host import BotReport, Host, start_and_run


def _print(reports: list[BotReport]) -> None:
    for r in reports:
        print(
            f"slot {r.slot:3d} player {r.player} t={r.last_tick // 20:5d}s "
            f"{r.age:6s} pop {r.population:3d} vill {r.villagers:3d} army {r.army:3d} "
            f"bld {r.buildings:3d} ok {r.accepted} rej {r.rejected} err {r.errors}"
        )
    print(flush=True)


def _write_report(path: str, reports: list[BotReport], started: float) -> None:
    """Atomically replace `path` with the bots' current reports."""
    document = {"seconds": round(time.monotonic() - started, 1), "bots": [asdict(r) for r in reports]}
    temporary = f"{path}.tmp"
    with open(temporary, "w", encoding="utf-8") as handle:
        json.dump(document, handle)
    os.replace(temporary, path)


def _host(args: argparse.Namespace) -> int:
    with open(args.tokens_file, encoding="utf-8") as handle:
        tokens = json.load(handle)
    started = time.monotonic()
    host = Host(args.url, tokens, Config())
    print(f"{len(tokens)} bots connected in {time.monotonic() - started:.1f}s", flush=True)
    started = time.monotonic()

    def progress(reports: list[BotReport]) -> None:
        _write_report(args.report, reports, started)

    reports = host.run(max_seconds=args.max_seconds, progress=progress, progress_every=10.0)
    _write_report(args.report, reports, started)
    return 1 if any(r.errors for r in reports) else 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="aoe-ai", description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)

    match = sub.add_parser("match", help="start a match with AI slots and play it")
    match.add_argument("--url", default="http://127.0.0.1:8080")
    match.add_argument("--admin-token", default=os.environ.get("AOE_MATCH_TOKEN", ""))
    match.add_argument("--bots", type=int, default=2, help="number of AI slots")
    match.add_argument("--humans", type=int, default=0, help="human slots (join them in the browser)")
    match.add_argument("--minutes", type=float, default=10.0, help="game minutes to play")
    match.add_argument("--tiles", type=int, default=500, help="map width in tiles (flat terrain)")
    match.add_argument("--seed", type=int, default=1)

    arena = sub.add_parser("arena", help="seeded lockstep matches against an arena server; prints JSON")
    arena.add_argument("--server-bin", required=True, help="the aoe-server binary")
    arena.add_argument("--bots", type=int, default=2)
    arena.add_argument("--minutes", type=float, default=30.0, help="game minutes per seed")
    arena.add_argument("--tiles", type=int, default=300, help="map width in tiles (flat terrain)")
    arena.add_argument("--seeds", default="1,2,3,4,5", help="comma-separated seeds, one match each")

    join = sub.add_parser("join", help="play one existing slot")
    join.add_argument("--url", default="http://127.0.0.1:8080")
    join.add_argument("--token", required=True, help="the slot's 48-digit token")
    join.add_argument("--minutes", type=float, default=10.0)

    hosting = sub.add_parser(
        "host", help="play many existing slots in this process (the scale harness uses it)"
    )
    hosting.add_argument("--url", default="http://127.0.0.1:8080")
    hosting.add_argument("--tokens-file", required=True, help="a JSON list of slot tokens")
    hosting.add_argument("--max-seconds", type=float, required=True, help="wall-clock time limit")
    hosting.add_argument("--report", required=True, help="JSON file rewritten every few seconds")

    args = parser.parse_args(argv)
    logging.basicConfig(level=logging.INFO, format="%(asctime)s %(name)s %(message)s")
    if args.command == "arena":
        from pathlib import Path

        from .arena import run_seeds

        root = Path(__file__).resolve().parents[2]
        seeds = [int(seed) for seed in args.seeds.split(",") if seed]
        results = run_seeds(Path(args.server_bin), root, seeds, args.bots, args.minutes, args.tiles)
        print(json.dumps({"results": results}, indent=1))
        return 1 if any(b["errors"] for r in results for b in r["bots"]) else 0
    if args.command == "match":
        if not args.admin_token:
            parser.error("--admin-token (or AOE_MATCH_TOKEN) is required to start a match")
        reports = start_and_run(
            args.url,
            args.admin_token,
            bots=args.bots,
            humans=args.humans,
            minutes=args.minutes,
            tiles_per_side=args.tiles,
            seed=args.seed,
            progress=_print,
        )
    elif args.command == "host":
        return _host(args)
    else:
        host = Host(args.url, [args.token], Config())
        reports = host.run(until_tick=int(args.minutes * 60 * 20), progress=_print)
    _print(reports)
    return 1 if any(r.errors for r in reports) else 0


if __name__ == "__main__":
    sys.exit(main())

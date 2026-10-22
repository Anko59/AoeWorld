"""``python -m aoe_ai``: start a match with AI slots, or join one."""

from __future__ import annotations

import argparse
import logging
import os
import sys

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

    join = sub.add_parser("join", help="play one existing slot")
    join.add_argument("--url", default="http://127.0.0.1:8080")
    join.add_argument("--token", required=True, help="the slot's 48-digit token")
    join.add_argument("--minutes", type=float, default=10.0)

    args = parser.parse_args(argv)
    logging.basicConfig(level=logging.INFO, format="%(asctime)s %(name)s %(message)s")
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
    else:
        host = Host(args.url, [args.token], Config())
        reports = host.run(until_tick=int(args.minutes * 60 * 20), progress=_print)
    _print(reports)
    return 1 if any(r.errors for r in reports) else 0


if __name__ == "__main__":
    sys.exit(main())

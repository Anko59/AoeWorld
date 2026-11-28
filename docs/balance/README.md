# Measuring long matches

`make balance-match` starts ordinary Dark Age villages on a local map package.
It uses the same in-process Rust AI and match setup as the server, via
[game score](../scale/game-score.md), and advances the game without a
real-time clock. The default runs eight seats and then 100 seats, in separate
processes. Both have a 120-game-minute target and a 90-wall-minute budget each.
One long-job slot is held for the sequence. The Make target runs at reduced
CPU priority.

Use a source-backed map package. `make game-map` prepares the pinned Anatolia
package, or supply a package already available locally.

```sh
nice -n 10 make balance-match BALANCE_MATCH_ARGS='--package-directory .cache/maps --map-hash <hash> --seed 1 --workers 4 --out .cache/balance/run'
```

Use `--seats`, `--game-minutes` and `--wall-minutes` to set the workload and
budget. Output directories can be reused for different seat counts. A match
refuses to overwrite an existing stream. Keep the packages and raw output
outside Git.

Each stream has a match header with the package, revision, clock and
technology catalog. It records the starting state, each complete game minute,
each first sighting and the final state. A footer gives the actual ending tick
and the reason. A wall-limited match provides a censored observation. A stream
without a footer is incomplete.

- Deliveries are cumulative amounts actually credited, in food, wood, gold and
  stone order. Subtract two samples and divide by their game-time difference
  for an income rate. The delivery counters exclude trade, Markets, relics,
  tribute and refunds. Stockpiles reflect all credits and spending.
- Home gold reports natural and starting-kit piles within 14 tiles of the
  original Town Center. It tracks the same pile IDs throughout the match.
  The surveyed amount can be reduced by any player and represents this area
  only. The gold stockpile belongs to the player.
- Age and research arrays give the first completed simulation tick. Use
  Dark, Feudal, Castle, Imperial for ages and the header catalog order for
  technologies. A null means the milestone had yet to be observed.
- The census counts completed standing Town Centers and living villagers and
  army, including garrisons. The army includes scouts, monks and Generals.
  Transports, fishing and trade units, Stewards and Vassals are excluded.
- Food upkeep is the current bill per game minute. Shortage means a
  payment was missed and the stockpile still has no food. Samples capture
  the state at the recorded tick.
- Contact is the first authoritative line of sight to another seat's living
  unit or public building, checked every tick for each seat independently.
  It can be a scout sighting before any fighting. The event records the
  observer's age, Town Centers, completed technologies, villagers and army
  in that tick. Unobserved contact stays null through the final sample.

The first completed measurements are in the [France baseline](france-baseline.md).

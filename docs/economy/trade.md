# Trade and the Wonder

Sources: DE DAT via aoe2techtree `data.json` (update 188329) for costs, hit
points, speeds and times; the gold formula from the openage reverse-engineering
notes (`reverse_engineering/game_mechanics/market.md`, The Conquerors) and the
AoE2 DE forum thread "An accurate formula for trade" for the work rates. Not
verified in the current client; check an in-game route before relying on the
Cog rate.

## Trade Cart and Trade Cog

- Trade Cart (DAT 128): Market, Feudal Age, 100 wood and 50 gold, 51 s, 70 hp,
  0/0 armour, speed 1.25, LOS 7. Trade Cog (DAT 17): Dock, Feudal Age, 100 wood
  and 50 gold, 36 s, 80 hp, 0/6 armour, speed 1.65, LOS 6.
- A trained trader starts a run by itself: it walks or sails between its home
  building and the farthest other own, complete building of the same kind and
  is paid on arrival at each end. With no partner it waits and looks again
  every two seconds. A destroyed or unfinished partner is replaced.
- Gold per leg: `2 * w * (d / size + 0.3) * d + 0.5`, floored, where
  `d = sqrt(max(0, dx - 5)^2 + max(0, dy - 5)^2)` is the centre distance in
  tiles, `size` the shorter map edge and `w` 0.23 (Cart) or 0.299 (Cog). Centres
  closer than 12 tiles are not a route. A run first walks to the named
  building (no pay) and is paid only for a leg it departed and then finished.
- Any manual order (move, attack, stop, delete) ends the run. `TradeOrder`
  (right-click an own Market or Dock with traders selected; Python
  `client.trade(unit, building)`) starts it again from that building.
- A run is not saved: a restored world restarts every trader from its nearest
  own building (`rebind_trade_units`).
- Caravan (+50% Cart speed) belongs to the technology track; the hook is
  `CARAVAN_CART_SPEED_PERMILLE` in `economy/trade/mod.rs`.

## Wonder

DAT 276: Imperial Age, 1000 wood, 1000 stone, 1000 gold, 3500 s for one builder,
4800 hp, 5x5, armour 3 melee / 10 pierce, LOS 8. The match has no victory
conditions yet, so a standing Wonder is only a building; a countdown hooks onto
`BuildingKind::Wonder` when conditions exist.

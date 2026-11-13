//! Single-lane bridge crossings: straight, diagonal and off-grid decks, mouths
//! reached through forest lanes, and a long march over three bridged rivers.
//! Rivers are typed hydrology segments; trees are ordinary wood nodes. The
//! shapes follow river crossings of generated France and Paris maps (narrow
//! deck, wooded banks), at fixture scale; they are synthetic masks.
use super::*;

/// A typed river centre line, `width` tiles wide.
fn line(from: (i32, i32), to: (i32, i32), width: i32) -> RiverSegment {
    let delta = [(to.0 - from.0) * 256, (to.1 - from.1) * 256];
    let length = (i64::from(delta[0]).pow(2) + i64::from(delta[1]).pow(2)).isqrt();
    RiverSegment {
        start: [from.0 * 256, from.1 * 256],
        end: [to.0 * 256, to.1 * 256],
        width_units: (width * 256) as u16,
        station_start: 0,
        station_end: length,
        source_delta: delta,
        source_station_delta: length,
    }
}

/// Typed rivers of the bridge fixtures, or `None` for other fixtures.
pub(super) fn rivers(name: &str) -> Option<Vec<RiverSegment>> {
    Some(match name {
        "quality-march" => vec![
            line((150, -2), (150, 652), 4),
            line((300, -2), (300, 652), 7),
            line((450, -2), (450, 652), 7),
        ],
        // East-west rivers 10 tiles apart: the army enters the strip
        // between them from the west, then turns south onto the bridge.
        "general-bridge" | "bridge-40" | "bridge-opposed" | "bridge-meet" => {
            vec![line((-1, 80), (258, 80), 6), line((60, 70), (258, 70), 6)]
        }
        // One river along x + y = 240; its only crossing is a 45-degree deck.
        "forest-bridge" | "forest-bridge-40" | "forest-bridge-opposed" => {
            vec![line((-2, 242), (242, -2), 6)]
        }
        // Three rivers, each crossed only by its own bridge.
        "long-bridges" | "long-bridges-40" => vec![
            line((150, -2), (150, 652), 6),
            line((280, -2), (330, 652), 6),
            line((450, -2), (450, 652), 6),
        ],
        // One long river; its only crossing lies 50 tiles off the straight line.
        // The same river with no crossing at all.
        "army-river" => vec![line((150, -2), (150, 652), 6)],
        // Seven tiles is wider than any ford can open, so the river stays sealed.
        "army-river-sealed" => vec![line((150, -2), (150, 652), 7)],
        _ => return None,
    })
}

pub(super) fn side_meters(name: &str) -> Option<u64> {
    (name == "quality-march" || name.starts_with("long-bridges") || name.starts_with("army-"))
        .then_some(1300)
}

fn water(f: &Fixture, tile: TileCoord) -> Option<bool> {
    let Terrain::Map { generator, .. } = &f.world.terrain else {
        return None;
    };
    let (sample, node) = generator
        .tile_and_node_with_cancel(tile, &|| false)
        .ok()??;
    Some(if sample.water != WaterKind::None {
        true
    } else {
        if !sample.passable || node.is_some() {
            return None;
        }
        false
    })
}

impl Fixture {
    /// The shortest deck along `step` whose ends are open land and whose
    /// interior is water, starting at most 12 steps before `near`.
    fn bridge_across(&mut self, near: (i32, i32), step: (i32, i32)) -> aoe_core::BridgeSpan {
        let span = (1..=12)
            .flat_map(|back| (2..=20).map(move |length| (back, length)))
            .map(|(back, length)| {
                let from = TileCoord::new(near.0 - step.0 * back, near.1 - step.1 * back);
                let to = TileCoord::new(from.x + step.0 * length, from.y + step.1 * length);
                aoe_core::BridgeSpan::new(from, to).unwrap()
            })
            .filter(|span| {
                span.tiles().into_iter().all(|tile| {
                    let end = tile == span.from || tile == span.to;
                    water(self, tile) == Some(!end)
                })
            })
            .min_by_key(|span| (span.tiles().len(), span.from.x, span.from.y))
            .expect("a bridgeable crossing");
        self.bridge(span.from, span.to);
        span
    }

    /// Trees on both banks beside a mouth, leaving a three-tile lane that
    /// leads straight onto the deck: a column threads them to reach it.
    fn wooded_mouths(&mut self, span: aoe_core::BridgeSpan) {
        let (dx, dy) = (span.to.x - span.from.x, span.to.y - span.from.y);
        let length = f64::from(dx * dx + dy * dy).sqrt();
        let (ux, uy) = (f64::from(dx) / length, f64::from(dy) / length);
        let deck = span.tiles();
        for (mouth, sign) in [(span.from, -1.0), (span.to, 1.0)] {
            for y in mouth.y - 14..=mouth.y + 14 {
                for x in mouth.x - 14..=mouth.x + 14 {
                    let (px, py) = (f64::from(x - mouth.x), f64::from(y - mouth.y));
                    let along = (px * ux + py * uy) * sign;
                    let lateral = (px * uy - py * ux).abs();
                    let tile = TileCoord::new(x, y);
                    if (-1.0..=12.0).contains(&along)
                        && lateral > 1.6
                        && lateral < 9.0
                        && !deck.contains(&tile)
                        && water(self, tile) == Some(false)
                    {
                        trees(&mut self.world, TileRect::from_xywh(x, y, 1, 1));
                    }
                }
            }
        }
    }

    fn forest_bridge(&mut self) {
        let span = self.bridge_across((120, 120), (1, 1));
        self.wooded_mouths(span);
    }

    fn long_bridges(&mut self) {
        let straight = self.bridge_across((150, 100), (1, 0));
        self.wooded_mouths(straight);
        let off_grid = self.bridge_across((300, 105), (2, 1));
        self.wooded_mouths(off_grid);
        let diagonal = self.bridge_across((450, 108), (1, 1));
        self.wooded_mouths(diagonal);
        // Forest belts between the rivers, each with one six-tile gap. The
        // 4,096-node overlay bound ends them 100 tiles past the route; going
        // round them would cost far more than threading the gap.
        for (x, gap) in [(215, 80), (380, 125), (530, 95)] {
            trees(&mut self.world, TileRect::from_xywh(x, 0, 3, gap));
            trees(
                &mut self.world,
                TileRect::from_xywh(x, gap + 6, 3, 200 - gap - 6),
            );
        }
    }

    /// Builds the bridge fixtures; false for any other name.
    pub(super) fn build_bridges(&mut self, name: &str) -> bool {
        match name {
            "quality-march" => {
                let span = self.bridge_across((300, 105), (1, 0));
                self.wooded_mouths(span);
                let span = self.bridge_across((450, 108), (1, 1));
                self.wooded_mouths(span);
                for (x, gap, width) in [(215, 80, 6), (380, 125, 6), (530, 95, 1)] {
                    trees(&mut self.world, TileRect::from_xywh(x, 0, 3, gap));
                    trees(
                        &mut self.world,
                        TileRect::from_xywh(x, gap + width, 3, 200 - gap - width),
                    );
                }
            }
            "general-bridge" | "bridge-40" | "bridge-opposed" | "bridge-meet" => {
                self.bridge(TileCoord::new(100, 75), TileCoord::new(100, 85));
            }
            "forest-bridge" | "forest-bridge-40" | "forest-bridge-opposed" => self.forest_bridge(),
            "long-bridges" | "long-bridges-40" => self.long_bridges(),
            "army-river-sealed" => return true,
            "army-river" => {
                let span = self.bridge_across((150, 150), (1, 1));
                self.wooded_mouths(span);
                return true;
            }
            _ => return false,
        }
        match name {
            // The only crossing is one straight single-lane bridge, as at the
            // mapped Paris river where a General's army queued at the mouth:
            // the army arrives along the bank and turns onto a ten-tile deck.
            "general-bridge" => self.general_army((26, 43), (30, 38), at(115, 120)),
            "bridge-40" => self.group(35, 43, 40, at(115, 120)),
            // Two mixed blocks reach opposite ends of the same deck together.
            "bridge-opposed" => {
                self.group(35, 43, 20, at(130, 150));
                self.group(90, 160, 20, at(30, 25));
            }
            // Two villagers walk south onto the deck as a Knight walks north
            // onto it; a militiaman follows the villagers a little later.
            "bridge-meet" => {
                self.move_unit(UnitKind::Villager, at(100, 74), at(100, 92));
                self.move_unit(UnitKind::Villager, at(99, 74), at(99, 92));
                self.move_unit(UnitKind::Knight, at(100, 89), at(101, 74));
                self.move_unit(UnitKind::Militia, at(90, 75), at(102, 92));
            }
            "forest-bridge" => self.general_army((26, 43), (30, 38), at(185, 190)),
            "forest-bridge-40" => self.group(35, 43, 40, at(185, 190)),
            "forest-bridge-opposed" => {
                self.group(50, 60, 20, at(180, 195));
                self.group(165, 175, 20, at(50, 50));
            }
            "long-bridges" => self.general_army((26, 103), (30, 98), at(610, 110)),
            "quality-march" => self.group(35, 103, 40, at(610, 110)),
            "long-bridges-40" => self.group(35, 103, 40, at(610, 110)),
            _ => unreachable!(),
        }
        true
    }
}

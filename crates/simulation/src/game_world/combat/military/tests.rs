use super::*;

#[test]
fn researched_los_matches_the_full_research_stats() {
    let lines = [
        Tech::FeudalAge,
        Tech::CastleAge,
        Tech::Bracer,
        Tech::BodkinArrow,
        Tech::Fletching,
    ];
    let line_bits = lines.iter().fold(0, |bits, t| bits | t.bit());
    for kind in UnitKind::ALL {
        for feudal in [false, true] {
            for subset in 0..1u32 << lines.len() {
                let chosen = lines
                    .iter()
                    .enumerate()
                    .filter(|(n, _)| subset & (1 << n) != 0)
                    .fold(0, |bits, (_, t)| bits | t.bit());
                // Every other technology on as well: none of them moves sight.
                for researched in [chosen, chosen | !line_bits] {
                    assert_eq!(
                        researched_los(kind, feudal, researched),
                        researched_stats(kind, feudal, researched).los,
                        "{kind:?} feudal={feudal} researched={researched:#x}"
                    );
                }
            }
        }
    }
}

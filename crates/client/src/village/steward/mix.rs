//! A percentage slider reserves its share and proportionally rescales the rest.
pub(super) fn percentage(mut mix: [u8; 5], role: usize, share: u8) -> Option<[u8; 5]> {
    *mix.get_mut(role)? = share;
    let total: u16 = mix
        .iter()
        .enumerate()
        .filter(|&(n, _)| n != role)
        .map(|(_, &v)| u16::from(v))
        .sum();
    let uniform = total == 0;
    let denominator = if uniform { 4 } else { total };
    let mut remainders = Vec::with_capacity(4);
    let mut assigned = u16::from(share);
    for (n, value) in mix.iter_mut().enumerate() {
        if n == role {
            continue;
        }
        let numerator = u16::from(100 - share) * if uniform { 1 } else { u16::from(*value) };
        *value = (numerator / denominator) as u8;
        assigned += u16::from(*value);
        remainders.push((std::cmp::Reverse(numerator % denominator), n));
    }
    remainders.sort_unstable();
    for (_, n) in remainders.into_iter().take(usize::from(100 - assigned)) {
        mix[n] += 1;
    }
    Some(mix)
}

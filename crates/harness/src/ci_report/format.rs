//! Number and text formats shared by the page and the event descriptions.

/// `9.70G`, `97.0M`, `970k`, `970`: three significant digits.
pub(super) fn si(value: f64) -> String {
    let (scaled, unit) = [(1e9, "G"), (1e6, "M"), (1e3, "k")]
        .into_iter()
        .find(|(size, _)| value.abs() >= *size)
        .map_or((value, ""), |(size, unit)| (value / size, unit));
    let decimals = match scaled.abs() {
        v if v >= 100.0 || unit.is_empty() => 0,
        v if v >= 10.0 => 1,
        _ => 2,
    };
    format!("{scaled:.decimals$}{unit}")
}

/// `1,045,308`.
pub(super) fn grouped(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// `+1.2%`, `−3.4%` (a true minus sign), `0.0%`.
pub(super) fn signed(percent: f64) -> String {
    let rounded = (percent * 10.0).round() / 10.0;
    match rounded {
        v if v > 0.0 => format!("+{v:.1}%"),
        v if v < 0.0 => format!("−{:.1}%", -v),
        _ => "0.0%".to_owned(),
    }
}

/// A row's name for people: `simulation::late_game_x4` becomes `late game 4×`.
pub(super) fn label(row: &str) -> String {
    let name = row.rsplit("::").next().unwrap_or(row);
    match name {
        "wasm_gzip" => return "client bundle, gzip".to_owned(),
        "wasm_raw" => return "client bundle, raw".to_owned(),
        _ => {}
    }
    name.split('_')
        .map(|word| match word {
            "x4" => "4×",
            other => other,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// `25 Nov` from a `2026-11-25…` timestamp.
pub(super) fn short_date(at: &str) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let month = at
        .get(5..7)
        .and_then(|m| m.parse::<usize>().ok())
        .and_then(|m| MONTHS.get(m.wrapping_sub(1)));
    let day = at.get(8..10).and_then(|d| d.parse::<u32>().ok());
    match (month, day) {
        (Some(month), Some(day)) => format!("{day} {month}"),
        _ => at.to_owned(),
    }
}

/// `25 Nov 12:32` from a `2026-11-25T12:32…` timestamp.
pub(super) fn stamp(at: &str) -> String {
    format!("{} {}", short_date(at), at.get(11..16).unwrap_or(""))
        .trim_end()
        .to_owned()
}

/// The first sentence of `text`, cut at 160 characters.
pub(super) fn first_sentence(text: &str) -> String {
    let sentence = text.find(". ").map_or(text, |end| &text[..=end]);
    if sentence.chars().count() > 160 {
        let cut: String = sentence.chars().take(160).collect();
        format!("{}…", cut.trim_end())
    } else {
        sentence.to_owned()
    }
}

/// Text safe inside HTML, attribute values included.
pub(super) fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

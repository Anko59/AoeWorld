//! A resumed skirmish says where its restore time went (13 seconds at 73,000
//! units were one number), on its "skirmish match created" line alone:
//! `restore_us`, the whole restore, and `restore_phases_us`, its parts.
use super::*;
use std::{collections::BTreeMap, sync::Mutex};
use tracing::{
    field::{Field, Visit},
    span,
};

const PARTS: [&str; 7] = [
    "buildings",
    "explored",
    "fog_and_vision",
    "jobs_and_orders",
    "resources",
    "rest",
    "units",
];

/// One log event's fields, as printed.
type Line = BTreeMap<String, String>;

#[derive(Clone, Default)]
struct Lines(Arc<Mutex<Vec<Line>>>);

struct Fields<'a>(&'a mut Line);

impl Visit for Fields<'_> {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0.insert(field.name().to_owned(), format!("{value:?}"));
    }
}

impl tracing::Subscriber for Lines {
    fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
        true
    }
    fn new_span(&self, _: &span::Attributes<'_>) -> span::Id {
        span::Id::from_u64(1)
    }
    fn record(&self, _: &span::Id, _: &span::Record<'_>) {}
    fn record_follows_from(&self, _: &span::Id, _: &span::Id) {}
    fn event(&self, event: &tracing::Event<'_>) {
        let mut line = Line::new();
        event.record(&mut Fields(&mut line));
        self.0.lock().expect("lines").push(line);
    }
    fn enter(&self, _: &span::Id) {}
    fn exit(&self, _: &span::Id) {}
}

fn created(line: &Line) -> bool {
    line.get("message")
        .is_some_and(|message| message.trim_matches('"') == "skirmish match created")
}

/// Plays a new skirmish, saves it and resumes the save. Returns what
/// `GET /skirmish` then shows and the resumed match's tick phases.
async fn play_save_and_resume() -> (String, BTreeMap<&'static str, u64>) {
    let directory = tempfile::tempdir().expect("directory");
    let mut state = state(None);
    state.map_package_directory = Some(directory.path().to_owned());
    let hash = small_map(&state).await;
    let headers = controller(&state).await;
    let request = |save| {
        Json(StartRequest {
            map_hash: hash.clone(),
            opponents: 0,
            save,
        })
    };
    let _ = start(State(state.clone()), headers.clone(), request(None))
        .await
        .expect("started");
    until(&state, Phase::Ready).await;
    let service = state.gameplay.read().await.clone();
    let saves = crate::gameplay_match::saves::directory(&state).expect("a save directory");
    let card = service
        .save_game(saves, "Timed".to_owned())
        .await
        .expect("saved");
    let _ = start(State(state.clone()), headers, request(Some(card.id)))
        .await
        .expect("resumed");
    until(&state, Phase::Ready).await;
    let shown = serde_json::to_string(&status(&state).await).expect("status");
    let service = state.gameplay.read().await.clone();
    (shown, service.take_tick_phase_micros())
}

#[test]
fn only_the_created_line_of_a_resumed_skirmish_shows_its_restore_phases() {
    let lines = Lines::default();
    let dispatch = tracing::Dispatch::new(lines.clone());
    let blocking = dispatch.clone();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        // The restore itself runs, and logs, on a blocking thread.
        .on_thread_start(move || {
            std::mem::forget(tracing::dispatcher::set_default(&blocking));
        })
        .build()
        .expect("runtime");
    let _logged = tracing::dispatcher::set_default(&dispatch);
    let (shown, tick) = runtime.block_on(play_save_and_resume());
    let lines = lines.0.lock().expect("lines").clone();
    let matches: Vec<&Line> = lines.iter().filter(|line| created(line)).collect();
    assert_eq!(matches.len(), 2, "a new match, then the resumed one");

    let (new, resumed) = (matches[0], matches[1]);
    let whole: u64 = resumed
        .get("restore_us")
        .unwrap_or_else(|| panic!("the line shows no restore time: {resumed:?}"))
        .parse()
        .expect("microseconds");
    let parts: BTreeMap<String, u64> =
        serde_json::from_str(&resumed["restore_phases_us"]).expect("a map of microseconds");
    assert_eq!(
        parts.keys().map(String::as_str).collect::<Vec<_>>(),
        PARTS,
        "every phase, even at zero"
    );
    // `rest` is what the six others leave of the whole, so only rounding each
    // part down to a microsecond may be missing.
    let sum: u64 = parts.values().sum();
    assert!(
        sum <= whole && whole - sum < PARTS.len() as u64,
        "{parts:?} add up to {sum} of {whole}"
    );
    let seconds: f64 = resumed["seconds"].parse().expect("seconds");
    assert!(
        whole as f64 / 1e6 <= seconds,
        "the restore is a part of the match build: {resumed:?}"
    );

    // And nothing else shows them.
    assert!(
        new.keys().all(|field| !field.starts_with("restore")),
        "a new match restores nothing: {new:?}"
    );
    for line in lines.iter().filter(|line| !created(line)) {
        let text = format!("{line:?}");
        assert!(!text.contains("jobs_and_orders"), "{text}");
    }
    assert!(!shown.contains("jobs_and_orders"), "{shown}");
    assert!(!tick.contains_key("jobs_and_orders"), "{tick:?}");
    assert_eq!(
        tick.get("other").copied().unwrap_or(0),
        0,
        "the first tick does not carry the restore: {tick:?}"
    );
}

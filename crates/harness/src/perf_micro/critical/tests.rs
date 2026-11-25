use super::*;

const IDLE: &str = "<rayon_core::registry::WorkerThread>::wait_until_cold";

fn job(item: &str) -> String {
    format!(
        "<rayon_core::job::StackJob<rayon_core::latch::SpinLatch, helper<IterMutProducer<(aoe_core::EntityId, &mut {item})>>> (uncompressed names hold parentheses) as rayon_core::job::Job>::execute"
    )
}

/// A thread's profile: `(caller, callee, inclusive Ir)` call edges and a total.
fn profile(thread: u32, total: u64, edges: &[(&str, &str, u64)]) -> (u32, Profile) {
    let mut text = format!(
        "# callgrind format\nversion: 1\npart: 1\nthread: {thread}\npositions: line\nevents: Ir Dr\n"
    );
    for (caller, callee, cost) in edges {
        text += &format!("\nfn={caller}\ncfn={callee}\ncalls=1 0\n7 {cost} 1\n");
    }
    text += &format!("\nfn=self_work\n3 5 1\n\ntotals: {total} 9\n");
    assert_eq!(thread_of(&text), Some(thread));
    (thread, callgrind::parse(&text).expect("profile"))
}

#[test]
fn labels_name_the_last_aoe_path() {
    assert_eq!(
        label(&job(
            "aoe_simulation::game_world::route_scheduler::batch::Search"
        )),
        "aoe_simulation::game_world::route_scheduler::batch::Search"
    );
    assert_eq!(
        label(
            "ForEachConsumer<<aoe_simulation::game_world::GameWorld>::run_searches::{closure#0}>"
        ),
        "aoe_simulation::game_world::GameWorld>::run_searches"
    );
    assert_eq!(label("<rayon_core::job::HeapJob<()>>::execute"), "other");
    // Not a path start: `maoe_x` belongs to another identifier.
    assert_eq!(label("maoe_x::y"), "other");
}

#[test]
fn critical_path_is_main_plus_busiest_worker_per_phase() {
    let (a, b) = (job("aoe_x::A"), job("aoe_x::B"));
    let threads = [
        profile(1, 1_000, &[]),
        // Phase A: 300 on thread 2, 500 on thread 3; B: 200 and 50.
        profile(2, 700, &[(IDLE, &a, 300), (IDLE, &b, 200)]),
        profile(3, 700, &[(IDLE, &a, 500), (IDLE, &b, 50)]),
        // A job run inside another's wait is nested (`'2`), counted in its
        // parent; a search for work is not a job.
        profile(
            4,
            700,
            &[
                (IDLE, &a, 100),
                (
                    "<rayon_core::registry::WorkerThread>::wait_until_cold'2",
                    &a,
                    90,
                ),
                (IDLE, "<rayon_core::registry::WorkerThread>::find_work", 40),
            ],
        ),
    ];
    let result = critical(&threads).expect("critical path");
    assert_eq!(result.main, 1_000);
    assert_eq!(result.total, 3_100);
    assert_eq!(result.workers, 3);
    assert_eq!(
        result.phases,
        BTreeMap::from([("aoe_x::A".to_owned(), 500), ("aoe_x::B".to_owned(), 200)])
    );
    assert_eq!(result.path(), 1_700);
    assert!(
        result
            .note()
            .contains("critical path 1700 = main thread 1000")
    );
}

#[test]
fn one_worker_leaves_only_the_main_thread_and_a_main_thread_is_required() {
    let alone = critical(&[profile(1, 900, &[])]).expect("serial");
    assert_eq!((alone.path(), alone.total), (900, 900));
    let with_idle_worker = critical(&[profile(1, 900, &[]), profile(2, 3, &[])]).expect("idle");
    assert_eq!(with_idle_worker.path(), 900);
    assert!(critical(&[profile(2, 3, &[])]).is_err());
}

#[test]
fn profiles_are_read_from_a_gungraun_directory() {
    let root = tempfile::tempdir().expect("dir");
    let dir = root.path().join("target/gungraun/aoe-x/bench/group/kernel");
    fs::create_dir_all(&dir).expect("dirs");
    assert!(read_dir(&dir).is_err());
    let a = job("aoe_x::A");
    for (thread, total, edges) in [(1, 800, vec![]), (2, 400, vec![(IDLE, a.as_str(), 250)])] {
        let mut text = format!("thread: {thread}\nevents: Ir Dr\n");
        for (caller, callee, cost) in &edges {
            text += &format!("fn={caller}\ncfn={callee}\ncalls=1 0\n7 {cost} 1\n");
        }
        text += &format!("totals: {total} 9\n");
        fs::write(dir.join(format!("callgrind.kernel.t{thread}.p1.out")), text).expect("file");
    }
    // A previous run's file does not count.
    fs::write(dir.join("callgrind.kernel.t2.p1.out.old"), "junk").expect("old");
    let found = locate(root.path(), "bench::group::kernel").expect("located");
    let result = read_dir(&found).expect("read");
    assert_eq!(
        (result.main, result.path(), result.total),
        (800, 1_050, 1_200)
    );
    assert!(locate(root.path(), "bench::group::missing").is_none());
}

#[test]
fn rows_are_informational_and_say_when_profiles_are_gone() {
    let root = tempfile::tempdir().expect("dir");
    let dir = root.path().join("target/gungraun/aoe-x/bench/group/kernel");
    fs::create_dir_all(&dir).expect("dirs");
    for thread in [1, 2] {
        let text = format!("thread: {thread}\nevents: Ir\ntotals: 100\n");
        fs::write(dir.join(format!("callgrind.kernel.t{thread}.p1.out")), text).expect("file");
    }
    let item = |module: &str, parts: usize| {
        serde_json::json!({
            "module_path": module, "project_root": root.path(),
            "profiles": [{"tool": "Callgrind", "summaries": {"parts": vec![0; parts]}}]
        })
        .to_string()
    };
    let ndjson = root.path().join("simulation.ndjson");
    let lines = [
        item("bench::group::kernel", 2),
        item("bench::group::gone", 2),
        item("bench::group::serial", 1),
    ];
    fs::write(&ndjson, lines.join("\n")).expect("ndjson");
    let rows = super::super::critical_rows(&ndjson);
    assert_eq!(rows.len(), 2, "a kernel not split by thread has no row");
    assert_eq!(
        rows[0].metric,
        "instructions::bench::group::kernel_critical"
    );
    assert_eq!((rows[0].observed, rows[0].baseline), (Some(100), None));
    assert_eq!(rows[1].observed, None);
    assert!(
        rows.iter()
            .all(|row| row.verdict == crate::perf::Verdict::Pass)
    );
    assert!(
        rows[1]
            .note
            .as_deref()
            .is_some_and(|note| note.contains("unavailable"))
    );
}

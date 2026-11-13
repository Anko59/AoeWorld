use crate::quality_metrics::{
    march::{self, OrderPath},
    video,
};

#[test]
fn diagnostic_frames_are_readable_pngs_and_reuse_the_cached_background() {
    let directory = tempfile::tempdir().unwrap();
    let scene = march::prepare("river", OrderPath::Single).unwrap();
    let starts: Vec<_> = scene
        .ids
        .iter()
        .map(|&id| scene.world.unit(id).unwrap().position)
        .collect();
    for tick in [100, 200] {
        video::frame(
            directory.path(),
            video::FrameSummary {
                scenario: "river",
                path: "single",
                arrived: 0,
            },
            tick,
            &scene.world,
            &scene.ids,
            &starts,
            scene.goal,
        )
        .unwrap();
        let file = std::fs::File::open(
            directory
                .path()
                .join(format!("river-single/frame-{tick:06}.png")),
        )
        .unwrap();
        let mut reader = png::Decoder::new(file).read_info().unwrap();
        let mut pixels = vec![0; reader.output_buffer_size()];
        let frame = reader.next_frame(&mut pixels).unwrap();
        assert_eq!((frame.width, frame.height), (768, 320));
        assert!(pixels.as_chunks::<3>().0.contains(&[240, 216, 63]));
        assert!(
            pixels.as_chunks::<3>().0.contains(&[14, 72, 29]),
            "starting-resource forest obstacles must appear in the measured map"
        );
    }
}

#[test]
fn invalid_inputs_empty_frame_cohort_returns_an_error() {
    let directory = tempfile::tempdir().unwrap();
    let scene = march::prepare("river", OrderPath::Single).unwrap();
    assert!(
        video::frame(
            directory.path(),
            video::FrameSummary {
                scenario: "river",
                path: "single",
                arrived: 0
            },
            100,
            &scene.world,
            &[],
            &[],
            scene.goal
        )
        .is_err()
    );
}

#[test]
fn diagnostic_bar_uses_recorded_arrivals_even_when_actors_are_away_from_the_goal() {
    use crate::quality_metrics::{stats::Movement, video::FrameSummary};
    let directory = tempfile::tempdir().unwrap();
    let scene = march::prepare("river", OrderPath::Group).unwrap();
    let starts: Vec<_> = scene
        .ids
        .iter()
        .map(|&id| scene.world.unit(id).unwrap().position)
        .collect();
    let mut movement = Movement::new(vec![1; scene.ids.len()]);
    for n in 0..scene.ids.len() {
        movement.arrive(n, 1);
    }
    video::frame(
        directory.path(),
        FrameSummary {
            scenario: "river",
            path: "formation",
            arrived: movement.arrived_count(),
        },
        100,
        &scene.world,
        &scene.ids,
        &starts,
        scene.goal,
    )
    .unwrap();
    let file =
        std::fs::File::open(directory.path().join("river-formation/frame-000100.png")).unwrap();
    let mut reader = png::Decoder::new(file).read_info().unwrap();
    let mut pixels = vec![0; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut pixels).unwrap();
    let last_row = &pixels[(frame.height as usize - 1) * frame.width as usize * 3..];
    assert!(
        last_row
            .as_chunks::<3>()
            .0
            .iter()
            .all(|p| *p == [80, 188, 255])
    );
}

#[test]
fn final_observation_frame_is_kept_between_periodic_samples() {
    let directory = tempfile::tempdir().unwrap();
    let mut scene = march::prepare("river", OrderPath::Single).unwrap();
    scene.limit = 1;
    let report = scene.measure("river", Some(directory.path())).unwrap();
    assert_eq!(report["observed_ticks"], 1);
    assert!(
        directory
            .path()
            .join("river-single/frame-000001.png")
            .is_file(),
        "the final state must be visible even when it is not a periodic sample"
    );
}

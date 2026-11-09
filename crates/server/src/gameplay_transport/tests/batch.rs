//! The writer of one gameplay socket: which frames reach the peer, in which
//! order, and in how many flushes. A tick queues several small frames for a
//! client, and every flush is a write to its socket.
use super::super::{SEND_TIMEOUT, SendFailure, write_messages};
use aoe_core::{Tick, TileRect};
use aoe_protocol::{CommandResult, FogState, GameplayServerMessage, decode_gameplay_server};
use axum::extract::ws::Message;
use futures_util::Sink;
use std::{
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
    time::{Duration, Instant},
};
use tokio::{
    sync::mpsc::{self, error::TrySendError},
    time::timeout,
};

/// The capacity `handle_socket` gives a session's queue.
const SESSION_QUEUE: usize = 64;

#[derive(Default)]
struct Record {
    frames: Vec<GameplayServerMessage>,
    /// `F` for a frame and `|` for a completed flush, in order: "FF|F|" is two
    /// frames in one flush, then a third in its own.
    shape: String,
}

/// How a socket stops cooperating.
#[derive(Clone, Copy)]
enum Trouble {
    /// Takes this many frames, then is never ready for another.
    FullAfter(usize),
    /// Takes this many frames, then refuses the next.
    BrokenAfter(usize),
    /// Takes frames and never finishes flushing them.
    NeverFlushes,
    /// Takes frames and fails to flush them.
    FailsToFlush,
}

/// The socket end of a session. Clones share one record, so a test keeps one
/// while the writer borrows the other.
#[derive(Clone, Default)]
struct Socket {
    record: Arc<Mutex<Record>>,
    trouble: Option<Trouble>,
}

impl Socket {
    fn with(trouble: Trouble) -> Self {
        Self {
            trouble: Some(trouble),
            ..Self::default()
        }
    }

    fn frames(&self) -> Vec<GameplayServerMessage> {
        self.record.lock().unwrap().frames.clone()
    }

    fn shape(&self) -> String {
        self.record.lock().unwrap().shape.clone()
    }

    fn flushes(&self) -> usize {
        self.shape().matches('|').count()
    }

    fn taken(&self) -> usize {
        self.record.lock().unwrap().frames.len()
    }
}

impl Sink<Message> for Socket {
    type Error = ();

    fn poll_ready(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), ()>> {
        match self.trouble {
            Some(Trouble::FullAfter(limit)) if self.taken() >= limit => Poll::Pending,
            _ => Poll::Ready(Ok(())),
        }
    }

    fn start_send(self: Pin<&mut Self>, frame: Message) -> Result<(), ()> {
        if let Some(Trouble::BrokenAfter(limit)) = self.trouble
            && self.taken() >= limit
        {
            return Err(());
        }
        let Message::Binary(bytes) = frame else {
            panic!("the writer sends binary frames only: {frame:?}");
        };
        let message = decode_gameplay_server(&bytes).expect("a frame the client can decode");
        let mut record = self.record.lock().unwrap();
        record.frames.push(message);
        record.shape.push('F');
        Ok(())
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), ()>> {
        match self.trouble {
            Some(Trouble::NeverFlushes) => Poll::Pending,
            Some(Trouble::FailsToFlush) => Poll::Ready(Err(())),
            _ => {
                self.record.lock().unwrap().shape.push('|');
                Poll::Ready(Ok(()))
            }
        }
    }

    fn poll_close(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), ()>> {
        Poll::Ready(Ok(()))
    }
}

fn ack(sequence: u64) -> GameplayServerMessage {
    GameplayServerMessage::CommandAck {
        sequence,
        result: CommandResult::Accepted,
        applied_tick: Tick(sequence),
    }
}

/// What `deliver` queues for one client in one tick, in its order.
fn tick_frames() -> Vec<GameplayServerMessage> {
    let (revision, tick) = (1, Tick(9));
    vec![
        GameplayServerMessage::FogState(FogState {
            revision,
            tick,
            region: TileRect::from_xywh(0, 0, 8, 8),
            disabled: true,
            tiles: vec![],
            overview: vec![],
        }),
        ack(1),
        GameplayServerMessage::Tick {
            revision,
            tick,
            changed_units: vec![],
            removals: vec![],
            changed_players: vec![],
            changed_buildings: vec![],
            building_removals: vec![],
        },
        GameplayServerMessage::Holdings {
            revision,
            tick,
            shapes: vec![],
            complete: true,
        },
        GameplayServerMessage::MoveQueues {
            revision,
            tick,
            queues: vec![],
        },
        GameplayServerMessage::WildlifeState {
            revision,
            tick,
            animals: vec![],
        },
        GameplayServerMessage::Projectiles {
            revision,
            tick,
            projectiles: vec![],
        },
    ]
}

type Queue = (
    mpsc::Sender<GameplayServerMessage>,
    mpsc::Receiver<GameplayServerMessage>,
);

/// A session's queue that already holds `messages`.
fn queue(messages: impl IntoIterator<Item = GameplayServerMessage>) -> Queue {
    let (sender, receiver) = mpsc::channel(SESSION_QUEUE);
    for message in messages {
        sender.try_send(message).expect("room in the queue");
    }
    (sender, receiver)
}

/// How many queued frames the writer has not taken.
fn waiting(sender: &mpsc::Sender<GameplayServerMessage>) -> usize {
    sender.max_capacity() - sender.capacity()
}

/// Set when the writer asks to run again without waiting for anything.
#[derive(Default)]
struct WokeItself(AtomicBool);

impl Wake for WokeItself {
    fn wake(self: Arc<Self>) {
        self.0.store(true, Ordering::Relaxed);
    }
}

/// Runs `writer` until it waits for something from outside (a frame, the
/// socket, the clock), as its task does between two wake-ups. One poll is
/// not enough for a writer that yields to the scheduler on its way.
async fn settle<F: Future + Unpin>(writer: &mut F) -> Poll<F::Output> {
    for _ in 0..SESSION_QUEUE {
        let woke_itself = Arc::new(WokeItself::default());
        let waker = Waker::from(woke_itself.clone());
        let polled = Pin::new(&mut *writer).poll(&mut Context::from_waker(&waker));
        // The scheduler delivers a yield's wake-up once this task yields too.
        tokio::task::yield_now().await;
        if polled.is_ready() || !woke_itself.0.load(Ordering::Relaxed) {
            return polled;
        }
    }
    panic!("the writer keeps waking itself instead of waiting");
}

/// Runs the writer over two frames, a world reset and two more frames, with
/// the queue still open. Returns the socket and what the queue still holds.
async fn written_up_to_a_reset() -> (Socket, Vec<GameplayServerMessage>) {
    let reset = GameplayServerMessage::WorldReset { world_id: 2 };
    let (_sender, mut receiver) = queue([ack(1), ack(2), reset, ack(4), ack(5)]);
    let socket = Socket::default();
    let mut wire = socket.clone();
    let outcome = {
        let mut writer = std::pin::pin!(write_messages(&mut wire, &mut receiver));
        settle(&mut writer).await
    };
    assert!(
        matches!(outcome, Poll::Ready(Ok(()))),
        "the reset ends the writer though the queue is still open: {outcome:?}"
    );
    let later = std::iter::from_fn(|| receiver.try_recv().ok()).collect();
    (socket, later)
}

#[tokio::test]
async fn the_frames_of_one_tick_leave_in_a_single_flush() {
    let frames = tick_frames();
    let (sender, mut receiver) = queue(frames.clone());
    drop(sender);
    let socket = Socket::default();
    let outcome = write_messages(&mut socket.clone(), &mut receiver).await;
    assert!(outcome.is_ok(), "{outcome:?}");
    assert_eq!(socket.frames(), frames, "every frame, in queue order");
    assert_eq!(
        socket.flushes(),
        1,
        "seven frames queued together are one flush, not seven"
    );
    assert_eq!(socket.shape(), "FFFFFFF|");
}

#[tokio::test]
async fn a_full_queue_leaves_in_a_single_flush() {
    let frames: Vec<_> = (1..=SESSION_QUEUE as u64).map(ack).collect();
    let (sender, mut receiver) = queue(frames.clone());
    drop(sender);
    let socket = Socket::default();
    let outcome = write_messages(&mut socket.clone(), &mut receiver).await;
    assert!(outcome.is_ok(), "{outcome:?}");
    assert_eq!(socket.frames(), frames, "every frame, in queue order");
    assert_eq!(socket.flushes(), 1, "a full queue is one flush");
    assert!(socket.shape().ends_with("F|"), "{}", socket.shape());
}

#[tokio::test]
async fn frames_queued_while_the_writer_waits_are_one_more_flush() {
    let (sender, mut receiver) = queue((1..=3).map(ack));
    let socket = Socket::default();
    let mut wire = socket.clone();
    let mut writer = std::pin::pin!(write_messages(&mut wire, &mut receiver));
    assert!(settle(&mut writer).await.is_pending());
    assert_eq!(socket.shape(), "FFF|", "three frames queued together");
    for sequence in 4..=7 {
        sender.try_send(ack(sequence)).unwrap();
    }
    assert!(settle(&mut writer).await.is_pending());
    assert_eq!(socket.frames(), (1..=7).map(ack).collect::<Vec<_>>());
    assert_eq!(
        socket.shape(),
        "FFF|FFFF|",
        "one flush per wake-up, however many frames were waiting"
    );
}

#[tokio::test]
async fn a_world_reset_shares_the_flush_of_the_frames_before_it() {
    let (socket, _) = written_up_to_a_reset().await;
    assert_eq!(
        socket.shape(),
        "FFF|",
        "two frames and the reset queued together are one flush"
    );
}

#[tokio::test]
async fn a_world_reset_ends_the_writer_and_leaves_later_frames_queued() {
    let (socket, later) = written_up_to_a_reset().await;
    let reset = GameplayServerMessage::WorldReset { world_id: 2 };
    assert_eq!(socket.frames(), [ack(1), ack(2), reset]);
    assert!(
        socket.shape().ends_with("F|"),
        "the reset is flushed before the writer ends: {}",
        socket.shape()
    );
    assert_eq!(later, [ack(4), ack(5)], "nothing after the reset is taken");
}

#[tokio::test]
async fn a_lone_frame_is_flushed_before_the_writer_waits_again() {
    let (sender, mut receiver) = queue([]);
    let socket = Socket::default();
    let mut wire = socket.clone();
    let mut writer = std::pin::pin!(write_messages(&mut wire, &mut receiver));
    assert!(settle(&mut writer).await.is_pending());
    assert_eq!(socket.shape(), "", "an idle writer leaves the socket alone");
    for (sequence, shape) in [(1, "F|"), (2, "F|F|")] {
        sender.try_send(ack(sequence)).unwrap();
        assert!(settle(&mut writer).await.is_pending());
        assert_eq!(socket.shape(), shape, "no frame waits for the next one");
    }
}

#[tokio::test]
async fn frames_the_socket_cannot_take_stay_in_the_bounded_queue() {
    let (sender, mut receiver) = queue((1..=10).map(ack));
    let socket = Socket::with(Trouble::FullAfter(3));
    let mut wire = socket.clone();
    let mut writer = std::pin::pin!(write_messages(&mut wire, &mut receiver));
    assert!(settle(&mut writer).await.is_pending());
    assert_eq!(socket.frames(), (1..=3).map(ack).collect::<Vec<_>>());
    assert_eq!(
        waiting(&sender),
        6,
        "the writer holds the one frame the socket was not ready for"
    );
}

#[tokio::test]
async fn an_unfinished_flush_takes_nothing_more_from_the_queue() {
    let (sender, mut receiver) = queue([ack(0)]);
    let socket = Socket::with(Trouble::NeverFlushes);
    let mut wire = socket.clone();
    let mut writer = std::pin::pin!(write_messages(&mut wire, &mut receiver));
    assert!(settle(&mut writer).await.is_pending());
    for sequence in 1..=SESSION_QUEUE as u64 {
        sender
            .try_send(ack(sequence))
            .expect("the writer took the only queued frame");
    }
    assert!(settle(&mut writer).await.is_pending());
    assert_eq!(socket.frames(), [ack(0)]);
    assert!(
        matches!(sender.try_send(ack(99)), Err(TrySendError::Full(_))),
        "the queue stays full, so producers see the backpressure"
    );
}

#[tokio::test]
async fn a_frame_that_cannot_be_encoded_ends_the_writer_after_the_frames_before_it() {
    let invalid = GameplayServerMessage::Projectiles {
        revision: 0,
        tick: Tick(0),
        projectiles: vec![],
    };
    let (_sender, mut receiver) = queue([ack(1), ack(2), invalid, ack(4)]);
    let socket = Socket::default();
    let outcome = write_messages(&mut socket.clone(), &mut receiver).await;
    assert!(
        matches!(outcome, Err(SendFailure::Encode(_))),
        "{outcome:?}"
    );
    assert_eq!(socket.frames(), [ack(1), ack(2)]);
    assert_eq!(receiver.try_recv().ok(), Some(ack(4)));
}

#[tokio::test]
async fn a_socket_that_refuses_a_frame_or_a_flush_ends_the_writer_with_a_write_failure() {
    let (_sender, mut receiver) = queue((1..=4).map(ack));
    let socket = Socket::with(Trouble::BrokenAfter(2));
    let outcome = write_messages(&mut socket.clone(), &mut receiver).await;
    assert!(matches!(outcome, Err(SendFailure::Write)), "{outcome:?}");
    assert_eq!(socket.frames(), [ack(1), ack(2)]);
    assert_eq!(
        receiver.try_recv().ok(),
        Some(ack(4)),
        "the refused frame was taken, the next was not"
    );

    let (_sender, mut receiver) = queue([ack(1)]);
    let mut socket = Socket::with(Trouble::FailsToFlush);
    let outcome = write_messages(&mut socket, &mut receiver).await;
    assert!(matches!(outcome, Err(SendFailure::Write)), "{outcome:?}");
}

/// Takes `SEND_TIMEOUT` of real time: this crate's tokio has no paused clock.
#[tokio::test]
async fn a_stalled_socket_ends_the_writer_at_the_send_timeout_and_an_idle_queue_does_not() {
    let stalled = |trouble: Trouble| async move {
        let (_sender, mut receiver) = queue((1..=3).map(ack));
        let mut socket = Socket::with(trouble);
        let started = Instant::now();
        // A writer that never gives up fails here instead of hanging the suite.
        let writer = write_messages(&mut socket, &mut receiver);
        let outcome = timeout(SEND_TIMEOUT * 2, writer).await;
        (outcome, started.elapsed())
    };
    let idle = async {
        let (_sender, mut receiver) = queue([]);
        let mut socket = Socket::default();
        let writer = write_messages(&mut socket, &mut receiver);
        timeout(SEND_TIMEOUT + Duration::from_millis(500), writer).await
    };
    let (unready, unready_later, unflushed, idle) = tokio::join!(
        stalled(Trouble::FullAfter(0)),
        stalled(Trouble::FullAfter(2)),
        stalled(Trouble::NeverFlushes),
        idle,
    );
    for (outcome, waited) in [unready, unready_later, unflushed] {
        assert!(
            matches!(outcome, Ok(Err(SendFailure::Timeout))),
            "{outcome:?}"
        );
        assert!(waited >= SEND_TIMEOUT, "gave up after {waited:?}");
    }
    assert!(
        idle.is_err(),
        "waiting for the next frame is not a slow write: {idle:?}"
    );
}

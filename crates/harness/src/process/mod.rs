//! Bounded child-process supervision with deadlines, cancellation, and retained failure logs.
use std::{
    collections::VecDeque,
    ffi::OsStr,
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const LOG_LIMIT: usize = 64 * 1024;

#[derive(Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);

impl Cancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
    fn cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProcessError {
    #[error("could not start {program}: {source}")]
    Start {
        program: String,
        #[source]
        source: io::Error,
    },
    #[error("{program} exited with {code:?}; log: {log}")]
    Exit {
        program: String,
        code: Option<i32>,
        log: String,
    },
    #[error("{program} exceeded its {seconds}s deadline; log: {log}")]
    Deadline {
        program: String,
        seconds: u64,
        log: String,
    },
    #[error("{program} was cancelled; log: {log}")]
    Cancelled { program: String, log: String },
    #[error("could not supervise {program}: {source}")]
    Monitor {
        program: String,
        #[source]
        source: io::Error,
    },
}

#[derive(Clone, Copy)]
struct ExitState {
    success: bool,
    code: Option<i32>,
}

trait Clock {
    fn elapsed(&self) -> Duration;
    fn sleep(&self, duration: Duration);
}

struct WallClock(Instant);

impl Clock for WallClock {
    fn elapsed(&self) -> Duration {
        self.0.elapsed()
    }
    fn sleep(&self, duration: Duration) {
        thread::sleep(duration);
    }
}

trait ProcessHandle {
    fn poll(&mut self) -> io::Result<Option<ExitState>>;
    fn terminate(&mut self) -> io::Result<()>;
}

struct RealChild(Child);

impl RealChild {
    #[cfg(unix)]
    fn kill_group(&self) {
        use nix::{
            sys::signal::{Signal, killpg},
            unistd::Pid,
        };
        let _ = killpg(Pid::from_raw(self.0.id() as i32), Signal::SIGKILL);
    }

    #[cfg(not(unix))]
    fn kill_group(&self) {}
}

impl ProcessHandle for RealChild {
    fn poll(&mut self) -> io::Result<Option<ExitState>> {
        let status = self.0.try_wait()?;
        if let Some(status) = status {
            self.kill_group();
            Ok(Some(ExitState {
                success: status.success(),
                code: status.code(),
            }))
        } else {
            Ok(None)
        }
    }

    fn terminate(&mut self) -> io::Result<()> {
        self.kill_group();
        let _ = self.0.kill();
        self.0.wait().map(|_| ())
    }
}

enum Outcome {
    Success,
    Failed(Option<i32>),
    Deadline,
    Cancelled,
}

fn wait_loop<C: Clock, P: ProcessHandle>(
    clock: &C,
    process: &mut P,
    deadline: Duration,
    cancellation: &Cancellation,
) -> io::Result<Outcome> {
    loop {
        if let Some(state) = process.poll()? {
            return Ok(if state.success {
                Outcome::Success
            } else {
                Outcome::Failed(state.code)
            });
        }
        if cancellation.cancelled() {
            process.terminate()?;
            return Ok(Outcome::Cancelled);
        }
        if clock.elapsed() >= deadline {
            process.terminate()?;
            return Ok(Outcome::Deadline);
        }
        clock.sleep(Duration::from_millis(25));
    }
}

struct BoundedLog {
    bytes: VecDeque<u8>,
    truncated: bool,
}

impl BoundedLog {
    fn new() -> Self {
        Self {
            bytes: VecDeque::with_capacity(LOG_LIMIT),
            truncated: false,
        }
    }
    fn push(&mut self, chunk: &[u8]) {
        for byte in chunk {
            if self.bytes.len() == LOG_LIMIT {
                self.bytes.pop_front();
                self.truncated = true;
            }
            self.bytes.push_back(*byte);
        }
    }
    fn render(self) -> Vec<u8> {
        let mut output = if self.truncated {
            b"[earlier output truncated]\n".to_vec()
        } else {
            Vec::new()
        };
        output.extend(self.bytes);
        output
    }
}

fn drain(mut input: impl Read) -> io::Result<Vec<u8>> {
    let mut log = BoundedLog::new();
    let mut buffer = [0u8; 4096];
    loop {
        let amount = input.read(&mut buffer)?;
        if amount == 0 {
            return Ok(log.render());
        }
        log.push(&buffer[..amount]);
    }
}

fn join(reader: thread::JoinHandle<io::Result<Vec<u8>>>) -> io::Result<Vec<u8>> {
    reader
        .join()
        .map_err(|_| io::Error::other("log reader panicked"))?
}

fn workspace_root() -> io::Result<PathBuf> {
    let current = std::env::current_dir()?;
    for ancestor in current.ancestors() {
        if let Ok(manifest) = fs::read_to_string(ancestor.join("Cargo.toml"))
            && manifest.lines().any(|line| line.trim() == "[workspace]")
        {
            return Ok(ancestor.to_path_buf());
        }
    }
    Ok(current)
}

fn retain(program: &str, stdout: &[u8], stderr: &[u8]) -> io::Result<PathBuf> {
    let directory = workspace_root()?.join("reports/process");
    fs::create_dir_all(&directory)?;
    let name = Path::new(program)
        .file_name()
        .and_then(|part| part.to_str())
        .unwrap_or("process");
    let safe: String = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_nanos();
    let path = directory.join(format!("{safe}-{}-{nonce}.log", std::process::id()));
    let mut file = fs::File::create(&path)?;
    file.write_all(b"--- stdout ---\n")?;
    file.write_all(stdout)?;
    file.write_all(b"\n--- stderr ---\n")?;
    file.write_all(stderr)?;
    Ok(path)
}

pub fn run(program: &str, args: &[&str], deadline: Duration) -> Result<(), ProcessError> {
    run_cancellable(program, args, deadline, &Cancellation::default())
}

pub fn run_cancellable(
    program: &str,
    args: &[&str],
    deadline: Duration,
    cancellation: &Cancellation,
) -> Result<(), ProcessError> {
    let mut command = Command::new(program);
    command
        .args(args.iter().map(OsStr::new))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = RealChild(command.spawn().map_err(|source| ProcessError::Start {
        program: program.into(),
        source,
    })?);
    let stdout = child.0.stdout.take().ok_or_else(|| ProcessError::Monitor {
        program: program.into(),
        source: io::Error::other("stdout pipe missing"),
    })?;
    let stderr = child.0.stderr.take().ok_or_else(|| ProcessError::Monitor {
        program: program.into(),
        source: io::Error::other("stderr pipe missing"),
    })?;
    let stdout_reader = thread::spawn(move || drain(stdout));
    let stderr_reader = thread::spawn(move || drain(stderr));
    let result = wait_loop(
        &WallClock(Instant::now()),
        &mut child,
        deadline,
        cancellation,
    );
    if result.is_err() {
        let _ = child.terminate();
    }
    let stdout = join(stdout_reader).map_err(|source| ProcessError::Monitor {
        program: program.into(),
        source,
    })?;
    let stderr = join(stderr_reader).map_err(|source| ProcessError::Monitor {
        program: program.into(),
        source,
    })?;
    let outcome = result.map_err(|source| ProcessError::Monitor {
        program: program.into(),
        source,
    })?;
    if matches!(outcome, Outcome::Success) {
        io::stderr()
            .write_all(&stdout)
            .map_err(|source| ProcessError::Monitor {
                program: program.into(),
                source,
            })?;
        io::stderr()
            .write_all(&stderr)
            .map_err(|source| ProcessError::Monitor {
                program: program.into(),
                source,
            })?;
        return Ok(());
    }
    let log = retain(program, &stdout, &stderr)
        .map(|path| path.display().to_string())
        .unwrap_or_else(|error| format!("unavailable ({error})"));
    match outcome {
        Outcome::Failed(code) => Err(ProcessError::Exit {
            program: program.into(),
            code,
            log,
        }),
        Outcome::Deadline => Err(ProcessError::Deadline {
            program: program.into(),
            seconds: deadline.as_secs(),
            log,
        }),
        Outcome::Cancelled => Err(ProcessError::Cancelled {
            program: program.into(),
            log,
        }),
        Outcome::Success => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    struct FakeClock(Cell<Duration>);
    impl Clock for FakeClock {
        fn elapsed(&self) -> Duration {
            self.0.get()
        }
        fn sleep(&self, duration: Duration) {
            self.0.set(self.0.get() + duration);
        }
    }
    struct FakeProcess {
        polls: usize,
        exit_after: usize,
        terminated: bool,
    }
    impl ProcessHandle for FakeProcess {
        fn poll(&mut self) -> io::Result<Option<ExitState>> {
            self.polls += 1;
            Ok((self.polls >= self.exit_after).then_some(ExitState {
                success: true,
                code: Some(0),
            }))
        }
        fn terminate(&mut self) -> io::Result<()> {
            self.terminated = true;
            Ok(())
        }
    }

    #[test]
    fn fake_deadline_and_cancellation_terminate() {
        let clock = FakeClock(Cell::new(Duration::ZERO));
        let mut process = FakeProcess {
            polls: 0,
            exit_after: usize::MAX,
            terminated: false,
        };
        assert!(matches!(
            wait_loop(
                &clock,
                &mut process,
                Duration::from_millis(50),
                &Cancellation::default()
            )
            .expect("wait"),
            Outcome::Deadline
        ));
        assert!(process.terminated);
        let cancel = Cancellation::default();
        cancel.cancel();
        let mut process = FakeProcess {
            polls: 0,
            exit_after: usize::MAX,
            terminated: false,
        };
        assert!(matches!(
            wait_loop(&clock, &mut process, Duration::from_secs(1), &cancel).expect("wait"),
            Outcome::Cancelled
        ));
        assert!(process.terminated);
    }

    #[test]
    fn bounded_log_keeps_tail() {
        let mut log = BoundedLog::new();
        log.push(&vec![b'a'; LOG_LIMIT]);
        log.push(b"tail");
        let rendered = log.render();
        assert!(rendered.starts_with(b"[earlier output truncated]"));
        assert!(rendered.ends_with(b"tail"));
    }

    #[test]
    fn real_exit_and_timeout_are_visible() {
        assert!(matches!(
            run("false", &[], Duration::from_secs(2)),
            Err(ProcessError::Exit { .. })
        ));
        assert!(matches!(
            run("sleep", &["2"], Duration::from_millis(10)),
            Err(ProcessError::Deadline { .. })
        ));
    }

    #[test]
    fn real_cancellation_stops_child_promptly() {
        let cancellation = Cancellation::default();
        let signal = cancellation.clone();
        let worker = thread::spawn(move || {
            thread::sleep(Duration::from_millis(30));
            signal.cancel();
        });
        let start = Instant::now();
        assert!(matches!(
            run_cancellable("sleep", &["5"], Duration::from_secs(10), &cancellation),
            Err(ProcessError::Cancelled { .. })
        ));
        worker.join().expect("cancellation worker");
        assert!(start.elapsed() < Duration::from_secs(2));
    }
}

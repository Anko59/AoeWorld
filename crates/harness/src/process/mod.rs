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

mod capture;
pub(crate) use capture::{CaptureExit, Captured, capture_command, capture_in, safe_observation};

const LOG_LIMIT: usize = 64 * 1024;

#[derive(Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);

impl Cancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
    pub(crate) fn cancelled(&self) -> bool {
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
    #[cfg(test)]
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

fn retain(program: &str, stdout: &[u8], stderr: &[u8], truncated: bool) -> io::Result<PathBuf> {
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
    if truncated {
        file.write_all(b"[output capture truncated]\n")?;
    }
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
    command.args(args.iter().map(OsStr::new));
    supervise(program, command, deadline, cancellation)
}

pub fn run_with_env(
    program: &str,
    args: &[&str],
    environment: &[(&str, &str)],
    deadline: Duration,
) -> Result<(), ProcessError> {
    let mut command = Command::new(program);
    command.args(args.iter().map(OsStr::new));
    for (name, value) in environment {
        command.env(name, value);
    }
    supervise(program, command, deadline, &Cancellation::default())
}

fn supervise(
    program: &str,
    command: Command,
    deadline: Duration,
    cancellation: &Cancellation,
) -> Result<(), ProcessError> {
    let captured = capture::capture_command(command, deadline, cancellation);
    if matches!(captured.exit, CaptureExit::Success) {
        if captured.truncated {
            io::stderr()
                .write_all(b"[output capture truncated]\n")
                .map_err(|source| ProcessError::Monitor {
                    program: program.into(),
                    source,
                })?;
        }
        io::stderr()
            .write_all(&captured.stdout)
            .and_then(|()| io::stderr().write_all(&captured.stderr))
            .map_err(|source| ProcessError::Monitor {
                program: program.into(),
                source,
            })?;
        return Ok(());
    }
    let log = match &captured.exit {
        CaptureExit::Start(_) | CaptureExit::Monitor(_) => String::new(),
        _ => retain(
            program,
            &captured.stdout,
            &captured.stderr,
            captured.truncated,
        )
        .map(|path| path.display().to_string())
        .unwrap_or_else(|error| format!("unavailable ({error})")),
    };
    match captured.exit {
        CaptureExit::Success => Ok(()),
        CaptureExit::Failed(code) => Err(ProcessError::Exit {
            program: program.into(),
            code,
            log,
        }),
        CaptureExit::Deadline => Err(ProcessError::Deadline {
            program: program.into(),
            seconds: deadline.as_secs(),
            log,
        }),
        CaptureExit::Cancelled => Err(ProcessError::Cancelled {
            program: program.into(),
            log,
        }),
        CaptureExit::Start(source) => Err(ProcessError::Start {
            program: program.into(),
            source,
        }),
        CaptureExit::Monitor(source) => Err(ProcessError::Monitor {
            program: program.into(),
            source,
        }),
    }
}

/// Execute against an explicit input root without changing the parent cwd.
pub fn run_in(
    root: &Path,
    program: &str,
    args: &[&str],
    environment: &[(&str, &str)],
    deadline: Duration,
) -> Result<(), ProcessError> {
    let command = capture::explicit_command(root, program, args, environment);
    supervise(program, command, deadline, &Cancellation::default())
}

#[cfg(test)]
mod tests;

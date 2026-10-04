use super::*;

#[derive(Debug)]
pub(crate) enum CaptureExit {
    Success,
    Failed(Option<i32>),
    Deadline,
    Cancelled,
    Start(io::Error),
    Monitor(io::Error),
}

/// Raw bounded tails on BOTH streams, including successful commands. No files
/// are written here; only the caller chooses a retention/evidence destination.
#[derive(Debug)]
pub(crate) struct Captured {
    pub(crate) exit: CaptureExit,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
    pub(crate) truncated: bool,
    pub(crate) duration: Duration,
}

pub(crate) fn capture_in(
    root: &Path,
    program: &str,
    args: &[&str],
    environment: &[(&str, &str)],
    deadline: Duration,
    cancellation: &Cancellation,
) -> Captured {
    capture_command(
        explicit_command(root, program, args, environment),
        deadline,
        cancellation,
    )
}

pub(super) fn explicit_command(
    root: &Path,
    program: &str,
    args: &[&str],
    environment: &[(&str, &str)],
) -> Command {
    let mut command = Command::new(program);
    command.current_dir(root).args(args);
    for (name, _) in std::env::vars_os() {
        if name.as_encoded_bytes().starts_with(b"GIT_") {
            command.env_remove(name);
        }
    }
    if program == "make" {
        for name in [
            "MAKEFLAGS",
            "MFLAGS",
            "MAKEOVERRIDES",
            "MAKEFILES",
            "GNUMAKEFLAGS",
            "CARGO_MAKEFLAGS",
        ] {
            command.env_remove(name);
        }
    }
    for (name, value) in environment {
        command.env(name, value);
    }
    command
}

#[cfg(unix)]
struct Pipe<R> {
    reader: R,
    log: BoundedLog,
    eof: bool,
}

#[cfg(unix)]
impl<R: Read + std::os::fd::AsFd> Pipe<R> {
    fn new(reader: R) -> io::Result<Self> {
        use nix::fcntl::{FcntlArg, OFlag, fcntl};
        let flags = fcntl(&reader, FcntlArg::F_GETFL).map_err(io::Error::from)?;
        fcntl(
            &reader,
            FcntlArg::F_SETFL(OFlag::from_bits_truncate(flags) | OFlag::O_NONBLOCK),
        )
        .map_err(io::Error::from)?;
        Ok(Self {
            reader,
            log: BoundedLog::new(),
            eof: false,
        })
    }

    fn poll(&mut self) -> io::Result<()> {
        if self.eof {
            return Ok(());
        }
        let mut buffer = [0u8; 4096];
        // A continuously writing process cannot starve deadline/cancellation.
        for _ in 0..16 {
            match self.reader.read(&mut buffer) {
                Ok(0) => {
                    self.eof = true;
                    break;
                }
                Ok(amount) => self.log.push(&buffer[..amount]),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;

pub(super) fn capture_command(
    mut command: Command,
    deadline: Duration,
    cancellation: &Cancellation,
) -> Captured {
    let started = Instant::now();
    let empty = |exit| Captured {
        exit,
        stdout: Vec::new(),
        stderr: Vec::new(),
        truncated: false,
        duration: started.elapsed(),
    };
    let stopped = || {
        if cancellation.cancelled() {
            Some(CaptureExit::Cancelled)
        } else if started.elapsed() >= deadline {
            Some(CaptureExit::Deadline)
        } else {
            None
        }
    };
    if let Some(exit) = stopped() {
        return empty(exit);
    }
    #[cfg(not(unix))]
    {
        let _ = (&mut command, deadline, cancellation);
        return empty(CaptureExit::Monitor(io::Error::new(
            io::ErrorKind::Unsupported,
            "bounded pipe capture requires Unix nonblocking descriptors",
        )));
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0);
        // Configuration time consumes budget too; never spawn when admission
        // is already cancelled/expired, even for a command that exits at once.
        if let Some(exit) = stopped() {
            return empty(exit);
        }
        let mut child = match command.spawn() {
            Ok(child) => RealChild(child),
            Err(error) => return empty(CaptureExit::Start(error)),
        };
        let pipes = (|| -> io::Result<_> {
            let stdout = child
                .0
                .stdout
                .take()
                .ok_or_else(|| io::Error::other("stdout pipe missing"))?;
            let stderr = child
                .0
                .stderr
                .take()
                .ok_or_else(|| io::Error::other("stderr pipe missing"))?;
            Ok((Pipe::new(stdout)?, Pipe::new(stderr)?))
        })();
        let (mut stdout, mut stderr) = match pipes {
            Ok(pipes) => pipes,
            Err(error) => {
                let _ = child.terminate();
                return empty(CaptureExit::Monitor(error));
            }
        };
        let mut exit = loop {
            if let Some(exit) = stopped() {
                break match child.terminate() {
                    Ok(()) => exit,
                    Err(error) => CaptureExit::Monitor(error),
                };
            }
            if let Err(error) = stdout.poll().and_then(|()| stderr.poll()) {
                let _ = child.terminate();
                break CaptureExit::Monitor(error);
            }
            // Cancellation/deadline wins over a late-observed successful exit.
            if let Some(exit) = stopped() {
                break match child.terminate() {
                    Ok(()) => exit,
                    Err(error) => CaptureExit::Monitor(error),
                };
            }
            match child.poll() {
                Ok(Some(state)) => {
                    if let Some(exit) = stopped() {
                        break exit;
                    }
                    break if state.success {
                        CaptureExit::Success
                    } else {
                        CaptureExit::Failed(state.code)
                    };
                }
                Ok(None) => (),
                Err(error) => {
                    let _ = child.terminate();
                    break CaptureExit::Monitor(error);
                }
            }
            thread::sleep(Duration::from_millis(10));
        };
        // Descendants may escape the owned process group and keep inherited FDs.
        // Never join a blocking reader: bounded nonblocking drain then close FDs.
        let drain_started = Instant::now();
        while !(stdout.eof && stderr.eof) && drain_started.elapsed() < Duration::from_millis(200) {
            if let Err(error) = stdout.poll().and_then(|()| stderr.poll()) {
                exit = CaptureExit::Monitor(error);
                break;
            }
            if !(stdout.eof && stderr.eof) {
                thread::sleep(Duration::from_millis(5));
            }
        }
        let truncated = stdout.log.truncated || stderr.log.truncated || !stdout.eof || !stderr.eof;
        Captured {
            exit,
            stdout: stdout.log.bytes.into_iter().collect(),
            stderr: stderr.log.bytes.into_iter().collect(),
            truncated,
            duration: started.elapsed(),
        }
    }
}

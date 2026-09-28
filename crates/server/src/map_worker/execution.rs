use std::{
    io::{Read, Write},
    path::Path,
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

const MAX_REQUEST_BYTES: usize = 64 * 1024;
const MAX_RESPONSE_BYTES: usize = 512 * 1024;
const MAX_ERROR_BYTES: usize = 8 * 1024;
const PREPARATION_DEADLINE: Duration = Duration::from_secs(2 * 60 * 60);

struct Worker {
    child: Child,
    terminated: bool,
}

impl Worker {
    fn terminate(&mut self) {
        if self.terminated {
            return;
        }
        self.terminated = true;
        #[cfg(unix)]
        {
            use nix::{
                sys::signal::{Signal, killpg},
                unistd::Pid,
            };
            if let Ok(pid) = i32::try_from(self.child.id()) {
                let _ = killpg(Pid::from_raw(pid), Signal::SIGKILL);
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.terminate();
    }
}

pub(super) fn execute(
    worker: &Path,
    input: Vec<u8>,
    cancelled: &AtomicBool,
    observe: impl FnMut(),
) -> Result<Vec<u8>, String> {
    execute_observed(worker, input, cancelled, PREPARATION_DEADLINE, observe)
}

pub(super) fn execute_with_deadline(
    worker: &Path,
    input: Vec<u8>,
    cancelled: &AtomicBool,
    deadline: Duration,
) -> Result<Vec<u8>, String> {
    execute_observed(worker, input, cancelled, deadline, || {})
}

fn execute_observed(
    worker: &Path,
    input: Vec<u8>,
    cancelled: &AtomicBool,
    deadline: Duration,
    mut observe: impl FnMut(),
) -> Result<Vec<u8>, String> {
    if input.len() > MAX_REQUEST_BYTES {
        return Err("map-worker request exceeds the configured bound".to_owned());
    }
    if cancelled.load(Ordering::Acquire) {
        return Err("map creation cancelled".to_owned());
    }
    let mut command = Command::new(worker);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = Worker {
        child: command
            .spawn()
            .map_err(|error| format!("could not start map worker: {error}"))?,
        terminated: false,
    };
    let mut stdin = child
        .child
        .stdin
        .take()
        .ok_or("map worker did not expose standard input")?;
    let stdout = child
        .child
        .stdout
        .take()
        .ok_or("map worker did not expose standard output")?;
    let stderr = child
        .child
        .stderr
        .take()
        .ok_or("map worker did not expose standard error")?;
    // Input may exceed the OS pipe capacity. Keep cancellation responsive even
    // when a broken worker never reads its request.
    let input_writer = thread::spawn(move || stdin.write_all(&input));
    let overflow = Arc::new(AtomicBool::new(false));
    let output_overflow = overflow.clone();
    let error_overflow = overflow.clone();
    let output_reader =
        thread::spawn(move || read_bounded(stdout, MAX_RESPONSE_BYTES, &output_overflow));
    let error_reader =
        thread::spawn(move || read_bounded(stderr, MAX_ERROR_BYTES, &error_overflow));
    let started = Instant::now();
    let outcome = loop {
        observe();
        if cancelled.load(Ordering::Acquire) {
            break Err("map creation cancelled".to_owned());
        }
        if overflow.load(Ordering::Acquire) {
            break Err("map-worker output exceeds the configured bound".to_owned());
        }
        if started.elapsed() >= deadline {
            break Err("map-worker preparation deadline exceeded".to_owned());
        }
        match child.child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => thread::sleep(Duration::from_millis(10)),
            Err(error) => break Err(format!("could not poll map worker: {error}")),
        }
    };
    // Descendants can inherit pipes even after the worker exits. Terminate the
    // entire owned process group before joining any pipe thread.
    child.terminate();
    let written = input_writer
        .join()
        .map_err(|_| "map-worker input writer panicked".to_owned());
    let output = join_reader(output_reader, "response");
    let error = join_reader(error_reader, "error");
    let status = outcome?;
    let output = output?;
    let error = error?;
    if overflow.load(Ordering::Acquire) {
        return Err("map-worker output exceeds the configured bound".to_owned());
    }
    if !status.success() {
        return Err(format!(
            "map worker failed: {}",
            String::from_utf8_lossy(&error).trim()
        ));
    }
    written?.map_err(|error| format!("could not send map-worker request: {error}"))?;
    Ok(output)
}

fn read_bounded(
    mut reader: impl Read,
    limit: usize,
    overflow: &AtomicBool,
) -> Result<Vec<u8>, String> {
    let mut output = Vec::new();
    let mut buffer = [0_u8; 8 * 1024];
    loop {
        let count = reader
            .read(&mut buffer)
            .map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        let available = limit.saturating_sub(output.len());
        output.extend_from_slice(&buffer[..count.min(available)]);
        if count > available {
            overflow.store(true, Ordering::Release);
        }
        // Continue draining until the owner terminates the process group. A
        // stopped reader would let a verbose worker block on a full pipe.
    }
    Ok(output)
}

fn join_reader(
    reader: thread::JoinHandle<Result<Vec<u8>, String>>,
    stream: &str,
) -> Result<Vec<u8>, String> {
    reader
        .join()
        .map_err(|_| format!("map-worker {stream} reader panicked"))?
        .map_err(|error| format!("could not read map-worker {stream}: {error}"))
}

#[cfg(test)]
mod tests;

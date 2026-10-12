#![cfg(unix)]
use super::*;
use std::{
    cell::Cell,
    os::fd::{AsFd, BorrowedFd},
};

struct Reader {
    descriptor: fs::File,
    calls: Cell<usize>,
    error: bool,
}
impl AsFd for Reader {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.descriptor.as_fd()
    }
}
impl Read for Reader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.calls.set(self.calls.get() + 1);
        if self.error {
            return Err(io::Error::other("fixture pipe read failed"));
        }
        buffer.fill(b'x');
        Ok(buffer.len())
    }
}

#[test]
fn continuously_readable_pipe_has_per_poll_quota_and_read_errors_surface() {
    let reader = Reader {
        descriptor: fs::File::open("/dev/null").unwrap(),
        calls: Cell::new(0),
        error: false,
    };
    let mut pipe = Pipe::new(reader).unwrap();
    pipe.poll().unwrap();
    assert_eq!(pipe.reader.calls.get(), 16);
    assert!(!pipe.eof);
    assert_eq!(pipe.log.bytes.len(), LOG_LIMIT);
    pipe.reader.error = true;
    assert!(
        pipe.poll()
            .unwrap_err()
            .to_string()
            .contains("fixture pipe read failed")
    );
}

#[test]
fn retained_stdout_keeps_early_failure_names_beyond_the_bounded_tail() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("stdout");
    let mut file = fs::File::create(&path).unwrap();
    let mut command = Command::new("sh");
    command.args([
        "-c",
        "printf 'early_failure\n'; head -c 100000 /dev/zero; printf 'last\n'; exit 1",
    ]);
    let captured = capture_command_retaining_stdout(
        command,
        Duration::from_secs(5),
        &Cancellation::default(),
        &mut file,
    );
    assert!(matches!(captured.exit, CaptureExit::Failed(Some(1))));
    assert!(captured.truncated);
    assert!(captured.stdout.ends_with(b"last\n"));
    let bytes = fs::read(path).unwrap();
    assert!(bytes.starts_with(b"early_failure\n"));
    assert!(bytes.ends_with(b"last\n"));
    assert!(bytes.len() > LOG_LIMIT);
}

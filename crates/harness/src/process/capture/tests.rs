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

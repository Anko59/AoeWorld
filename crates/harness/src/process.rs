use std::{
    ffi::OsStr,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[derive(Debug, thiserror::Error)]
pub enum ProcessError {
    #[error("could not start {program}: {source}")]
    Start {
        program: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{program} exited with {code:?}")]
    Exit { program: String, code: Option<i32> },
    #[error("{program} exceeded its {seconds}s deadline")]
    Deadline { program: String, seconds: u64 },
    #[error("could not monitor {program}: {source}")]
    Monitor {
        program: String,
        #[source]
        source: std::io::Error,
    },
}

pub fn run(program: &str, args: &[&str], deadline: Duration) -> Result<(), ProcessError> {
    let mut child = Command::new(program)
        .args(args.iter().map(OsStr::new))
        .stdin(Stdio::null())
        .spawn()
        .map_err(|source| ProcessError::Start {
            program: program.into(),
            source,
        })?;
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(|source| ProcessError::Monitor {
            program: program.into(),
            source,
        })? {
            return if status.success() {
                Ok(())
            } else {
                Err(ProcessError::Exit {
                    program: program.into(),
                    code: status.code(),
                })
            };
        }
        if start.elapsed() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(ProcessError::Deadline {
                program: program.into(),
                seconds: deadline.as_secs(),
            });
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_and_timeout_are_visible() {
        assert!(matches!(
            run("false", &[], Duration::from_secs(2)),
            Err(ProcessError::Exit { .. })
        ));
        assert!(matches!(
            run("sleep", &["2"], Duration::from_millis(10)),
            Err(ProcessError::Deadline { .. })
        ));
    }
}

use super::*;
use crate::process::Cancellation;

/// Signal observation is scoped to the synchronous gate-run CLI thread. Linux
/// signalfd avoids unsafe signal handlers; children are stopped by owned groups.
pub(super) struct Signals {
    #[cfg(target_os = "linux")]
    old: nix::sys::signal::SigSet,
    #[cfg(target_os = "linux")]
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    #[cfg(target_os = "linux")]
    worker: Option<std::thread::JoinHandle<()>>,
}
impl Signals {
    pub(super) fn new(cancellation: &Cancellation) -> Result<Self> {
        #[cfg(not(target_os = "linux"))]
        {
            let _ = cancellation;
            return Err("gate-run signal observation currently requires Linux".into());
        }
        #[cfg(target_os = "linux")]
        {
            use nix::sys::{
                signal::{SigSet, SigmaskHow, Signal, pthread_sigmask},
                signalfd::{SfdFlags, SignalFd},
            };
            use std::sync::{
                Arc,
                atomic::{AtomicBool, Ordering},
            };
            let mut signals = SigSet::empty();
            signals.add(Signal::SIGINT);
            signals.add(Signal::SIGTERM);
            let mut old = SigSet::empty();
            pthread_sigmask(SigmaskHow::SIG_BLOCK, Some(&signals), Some(&mut old))?;
            let fd = match SignalFd::with_flags(
                &signals,
                SfdFlags::SFD_NONBLOCK | SfdFlags::SFD_CLOEXEC,
            ) {
                Ok(fd) => fd,
                Err(error) => {
                    pthread_sigmask(SigmaskHow::SIG_SETMASK, Some(&old), None)?;
                    return Err(error.into());
                }
            };
            let stop = Arc::new(AtomicBool::new(false));
            let stopped = stop.clone();
            let cancellation = cancellation.clone();
            let worker = std::thread::spawn(move || {
                while !stopped.load(Ordering::SeqCst) {
                    match fd.read_signal() {
                        Ok(Some(_)) | Err(_) => cancellation.cancel(),
                        Ok(None) => (),
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
            });
            Ok(Self {
                old,
                stop,
                worker: Some(worker),
            })
        }
    }
}
impl Drop for Signals {
    fn drop(&mut self) {
        #[cfg(target_os = "linux")]
        {
            use nix::sys::signal::{SigmaskHow, pthread_sigmask};
            self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
            let _ = pthread_sigmask(SigmaskHow::SIG_SETMASK, Some(&self.old), None);
        }
    }
}

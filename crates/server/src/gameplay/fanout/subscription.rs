//! One ordered in-flight view and one replaceable latest view per session.
use aoe_protocol::GameplayServerMessage;
use std::sync::{Arc, Mutex};
use tokio::{
    sync::{Notify, mpsc},
    task::JoinHandle,
};

type View = Box<dyn Iterator<Item = GameplayServerMessage> + Send>;

pub(crate) struct Delivery {
    latest: Arc<Mutex<Option<View>>>,
    wake: Arc<Notify>,
    worker: JoinHandle<()>,
    busy: Arc<std::sync::atomic::AtomicBool>,
}

impl Drop for Delivery {
    fn drop(&mut self) {
        // A replaced/disconnected session must release its sender and views.
        self.worker.abort();
    }
}

impl Delivery {
    pub(crate) fn pending(&self) -> bool {
        self.busy.load(std::sync::atomic::Ordering::Acquire)
    }

    pub(crate) fn offer(
        held: &mut Option<Self>,
        sender: &mpsc::Sender<GameplayServerMessage>,
        mut view: View,
    ) -> bool {
        if let Some(delivery) = held {
            let mut latest = delivery
                .latest
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            *latest = Some(view);
            delivery
                .busy
                .store(true, std::sync::atomic::Ordering::Release);
            delivery.wake.notify_one();
            return true;
        }
        let mut waiting = None;
        for message in view.by_ref() {
            match sender.try_send(message) {
                Ok(()) => {}
                Err(mpsc::error::TrySendError::Full(message)) => {
                    waiting = Some(message);
                    break;
                }
                Err(mpsc::error::TrySendError::Closed(_)) => return false,
            }
        }
        let Some(first) = waiting else {
            return true;
        };
        let latest = Arc::new(Mutex::new(None::<View>));
        let wake = Arc::new(Notify::new());
        let busy = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let (next, notified, active) = (latest.clone(), wake.clone(), busy.clone());
        let sender = sender.clone();
        let worker = tokio::spawn(async move {
            if sender.send(first).await.is_err() {
                return;
            }
            loop {
                for message in view {
                    if sender.send(message).await.is_err() {
                        return;
                    }
                }
                loop {
                    {
                        let mut latest = next.lock().unwrap_or_else(|error| error.into_inner());
                        if let Some(pending) = latest.take() {
                            view = pending;
                            break;
                        }
                        active.store(false, std::sync::atomic::Ordering::Release);
                    }
                    notified.notified().await;
                }
            }
        });
        *held = Some(Self {
            latest,
            wake,
            worker,
            busy,
        });
        true
    }
}

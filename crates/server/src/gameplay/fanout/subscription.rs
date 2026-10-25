//! One ordered in-flight view and one replaceable latest view per session.
use aoe_protocol::GameplayServerMessage;
use std::sync::{Arc, Mutex};
use tokio::{
    sync::{Notify, mpsc},
    task::JoinHandle,
};

type View = Vec<GameplayServerMessage>;

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
        view: View,
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
        let mut waiting = Vec::new();
        for message in view {
            if !waiting.is_empty() {
                waiting.push(message);
                continue;
            }
            match sender.try_send(message) {
                Ok(()) => {}
                Err(mpsc::error::TrySendError::Full(message)) => waiting.push(message),
                Err(mpsc::error::TrySendError::Closed(_)) => return false,
            }
        }
        if waiting.is_empty() {
            return true;
        }
        let latest = Arc::new(Mutex::new(None::<View>));
        let wake = Arc::new(Notify::new());
        let busy = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let (next, notified, active) = (latest.clone(), wake.clone(), busy.clone());
        let sender = sender.clone();
        let worker = tokio::spawn(async move {
            loop {
                for message in waiting {
                    if sender.send(message).await.is_err() {
                        return;
                    }
                }
                loop {
                    {
                        let mut latest = next.lock().unwrap_or_else(|error| error.into_inner());
                        if let Some(view) = latest.take() {
                            waiting = view;
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

use std::sync::Mutex;

use rsnano_utils::sync::backpressure_channel::{self, Receiver, Sender};

use crate::{
    consensus::AecFact,
    utils::{BackpressureEventProcessor, spawn_backpressure_processor},
};

pub(crate) struct AecDelivery {
    sender: Mutex<Option<Sender<AecFact>>>,
    receiver: Mutex<Option<Receiver<AecFact>>>,
}

impl AecDelivery {
    pub(crate) fn new(queue_soft_limit: usize) -> Self {
        let (sender, receiver) = backpressure_channel::channel(queue_soft_limit);
        Self {
            sender: Mutex::new(Some(sender)),
            receiver: Mutex::new(Some(receiver)),
        }
    }

    pub(crate) fn queue_len(&self) -> usize {
        self.sender
            .lock()
            .unwrap()
            .as_ref()
            .map(|sender| sender.len())
            .unwrap_or(0)
    }

    pub(crate) fn start_event_processor<T>(&self, thread_name: impl Into<String>, processor: T)
    where
        T: BackpressureEventProcessor<AecFact> + Send + 'static,
    {
        let receiver = self
            .receiver
            .lock()
            .unwrap()
            .take()
            .expect("AEC event processor already started");

        spawn_backpressure_processor(thread_name, receiver, processor);
    }

    pub(crate) fn publish(&self, event: AecFact) {
        if let Some(sender) = self.sender.lock().unwrap().as_ref() {
            let _ = sender.send(event);
        }
    }

    pub(crate) fn stop(&self) {
        self.sender.lock().unwrap().take();
    }

    #[cfg(test)]
    pub(crate) fn try_recv(&self) -> Result<AecFact, std::sync::mpsc::TryRecvError> {
        self.receiver
            .lock()
            .unwrap()
            .as_ref()
            .expect("AEC delivery receiver unavailable")
            .try_recv()
    }
}

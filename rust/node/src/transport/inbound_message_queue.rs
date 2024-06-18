use super::ChannelEnum;
use rsnano_messages::DeserializedMessage;
use std::{
    collections::VecDeque,
    sync::{Arc, Condvar, Mutex},
};

pub struct InboundMessageQueue {
    max_entries: usize,
    state: Mutex<State>,
    producer_condition: Condvar,
    consumer_condition: Condvar,
    blocked: Option<Box<dyn Fn() + Send + Sync>>,
}

impl InboundMessageQueue {
    pub fn new(incoming_connections_max: usize) -> Self {
        Self {
            max_entries: incoming_connections_max * MAX_ENTRIES_PER_CONNECTION + 1,
            state: Mutex::new(State {
                entries: VecDeque::new(),
                stopped: false,
            }),
            producer_condition: Condvar::new(),
            consumer_condition: Condvar::new(),
            blocked: None,
        }
    }

    pub fn put(&self, message: DeserializedMessage, channel: Arc<ChannelEnum>) {
        {
            let mut lock = self.state.lock().unwrap();
            while lock.entries.len() >= self.max_entries && !lock.stopped {
                if let Some(callback) = &self.blocked {
                    callback();
                }
                lock = self.producer_condition.wait(lock).unwrap();
            }
            lock.entries.push_back((message, channel));
        }
        self.consumer_condition.notify_one();
    }

    pub fn next(&self) -> Option<(DeserializedMessage, Arc<ChannelEnum>)> {
        let result = {
            let mut lock = self.state.lock().unwrap();
            while lock.entries.is_empty() && !lock.stopped {
                lock = self.consumer_condition.wait(lock).unwrap();
            }
            if !lock.entries.is_empty() {
                Some(lock.entries.pop_front().unwrap())
            } else {
                None
            }
        };
        self.producer_condition.notify_one();
        result
    }

    pub fn size(&self) -> usize {
        self.state.lock().unwrap().entries.len()
    }

    /// Stop container and notify waiting threads
    pub fn stop(&self) {
        {
            let mut lock = self.state.lock().unwrap();
            lock.stopped = true;
        }
        self.consumer_condition.notify_all();
        self.producer_condition.notify_all();
    }
}

impl Default for InboundMessageQueue {
    fn default() -> Self {
        Self::new(2048)
    }
}

const MAX_ENTRIES_PER_CONNECTION: usize = 16;

struct State {
    entries: VecDeque<(DeserializedMessage, Arc<ChannelEnum>)>,
    stopped: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsnano_messages::Message;
    use std::thread::spawn;

    #[test]
    fn put_and_get_one_message() {
        let manager = InboundMessageQueue::new(1);
        assert_eq!(manager.size(), 0);
        manager.put(
            DeserializedMessage::new(Message::BulkPush, Default::default()),
            Arc::new(ChannelEnum::new_null()),
        );
        assert_eq!(manager.size(), 1);
        assert!(manager.next().is_some());
        assert_eq!(manager.size(), 0);
    }

    #[test]
    fn block_when_max_entries_reached() {
        let mut manager = InboundMessageQueue::new(1);
        let blocked_notification = Arc::new((Mutex::new(false), Condvar::new()));
        let blocked_notification2 = blocked_notification.clone();
        manager.blocked = Some(Box::new(move || {
            let (mutex, condvar) = blocked_notification2.as_ref();
            let mut lock = mutex.lock().unwrap();
            *lock = true;
            condvar.notify_one();
        }));
        let manager = Arc::new(manager);

        let message = DeserializedMessage::new(Message::BulkPush, Default::default());
        let channel = Arc::new(ChannelEnum::new_null());

        // Fill the queue
        for _ in 0..manager.max_entries {
            manager.put(message.clone(), channel.clone());
        }

        assert_eq!(manager.size(), manager.max_entries);

        // This task will wait until a message is consumed
        let manager_clone = manager.clone();
        let handle = spawn(move || {
            manager_clone.put(message, channel);
        });

        let (mutex, condvar) = blocked_notification.as_ref();
        let mut lock = mutex.lock().unwrap();
        while !*lock {
            lock = condvar.wait(lock).unwrap();
        }

        assert_eq!(manager.size(), manager.max_entries);
        manager.next();
        assert!(handle.join().is_ok());
        assert_eq!(manager.size(), manager.max_entries);
    }
}
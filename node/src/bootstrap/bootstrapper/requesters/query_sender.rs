use std::{sync::Arc, time::Duration};

use rand::RngCore;

use rsnano_messages::{AscPullReq, Message};
use rsnano_network::TrafficType;
use rsnano_nullable_clock::SteadyClock;
use rsnano_output_tracker::{OutputListenerMt, OutputTrackerMt};
use rsnano_utils::stats::{DetailType, StatType, Stats};

use crate::{
    bootstrap::bootstrapper::{
        query_tracker::{QueryTracker, RunningQuery},
        AscPullQuerySpec,
    },
    transport::MessageSender,
};
use tracing::trace;

/// Sends an AscPullReq message
pub(crate) struct QuerySender {
    message_sender: MessageSender,
    clock: SteadyClock,
    request_timeout: Duration,
    stats: Arc<Stats>,
    send_listener: OutputListenerMt<AscPullQuerySpec>,
    query_tracker: Arc<QueryTracker>,
}

impl QuerySender {
    pub(crate) fn new(
        message_sender: MessageSender,
        stats: Arc<Stats>,
        query_tracker: Arc<QueryTracker>,
    ) -> Self {
        Self::new_impl(message_sender, SteadyClock::default(), stats, query_tracker)
    }
    fn new_impl(
        message_sender: MessageSender,
        clock: SteadyClock,
        stats: Arc<Stats>,
        query_tracker: Arc<QueryTracker>,
    ) -> Self {
        Self {
            message_sender,
            clock,
            stats,
            query_tracker,
            request_timeout: Duration::from_secs(15),
            send_listener: OutputListenerMt::new(),
        }
    }

    pub fn set_request_timeout(&mut self, timeout: Duration) {
        self.request_timeout = timeout;
    }

    pub fn send(&mut self, spec: AscPullQuerySpec) -> bool {
        if self.send_listener.is_tracked() {
            self.send_listener.emit(spec.clone());
        }

        let id = rand::rng().next_u64();
        let now = self.clock.now();
        let query_type = spec.query_type();
        let channel_id = spec.channel.channel_id();
        let mut query = RunningQuery::from_spec(id, &spec, now, self.request_timeout);

        let message = Message::AscPullReq(AscPullReq {
            id,
            req_type: spec.req_type,
        });

        let sent =
            self.message_sender
                .try_send(&spec.channel, &message, TrafficType::BootstrapRequests);

        if sent {
            trace!(query_id = id, ?query_type, ?channel_id, "Pull request sent");
            self.stats.inc(StatType::Bootstrap, DetailType::Request);
            self.stats
                .inc(StatType::BootstrapRequest, query_type.into());

            // After the request has been sent, the peer has a limited time to respond
            query.response_cutoff = now + self.request_timeout;
            self.query_tracker.insert(query);

            true
        } else {
            trace!(
                query_id = id,
                ?query_type,
                ?channel_id,
                "Pull request failed"
            );

            self.stats
                .inc(StatType::Bootstrap, DetailType::RequestFailed);
            false
        }
    }

    #[allow(dead_code)]
    pub fn track(&self) -> Arc<OutputTrackerMt<AscPullQuerySpec>> {
        self.send_listener.track()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{bootstrap::bootstrapper::bootstrap_queue::BootstrapQueue, transport::SendEvent};
    use rsnano_nullable_clock::Timestamp;
    use rsnano_output_tracker::OutputTrackerMt;

    #[test]
    fn send_message() {
        let mut fixture = create_fixture();

        let spec = AscPullQuerySpec::new_test_instance();
        let channel_id = spec.channel.channel_id();

        let sent = fixture.query_sender.send(spec);
        assert!(sent);

        let output = fixture.send_tracker.output();
        assert_eq!(output.len(), 1, "no message sent!");
        assert_eq!(output[0].channel_id, channel_id);
        assert_eq!(output[0].traffic_type, TrafficType::BootstrapRequests);
        let Message::AscPullReq(_) = &output[0].message else {
            panic!("no asc pull req!")
        };
    }

    #[test]
    fn insert_into_running_queries() {
        let mut fixture = create_fixture();

        let spec = AscPullQuerySpec::new_test_instance();
        let bootstrap_queue = Arc::new(BootstrapQueue::new_null());
        bootstrap_queue.enqueue(spec.account);

        fixture.query_sender.send(spec.clone());

        assert_eq!(fixture.query_tracker.query_count(), 1);
        assert_eq!(
            fixture.query_tracker.front().unwrap().response_cutoff,
            fixture.now + fixture.query_sender.request_timeout
        );
    }

    #[test]
    fn when_channel_unavailable_should_not_send() {
        let mut fixture = create_fixture();
        let spec = AscPullQuerySpec::new_test_instance();
        let bootstrap_queue = Arc::new(BootstrapQueue::new_null());

        spec.channel.close();
        let sent = fixture.query_sender.send(spec.clone());

        assert!(!sent);
        assert_eq!(fixture.query_tracker.query_count(), 0);
        assert_eq!(bootstrap_queue.info().download_queue, 0);
        assert_eq!(bootstrap_queue.info().blocked, 0);
    }

    #[test]
    fn can_track_sends() {
        let mut fixture = create_fixture();
        let spec = AscPullQuerySpec::new_test_instance();

        let tracker = fixture.query_sender.track();
        fixture.query_sender.send(spec.clone());

        assert_eq!(tracker.output(), [spec]);
    }

    fn create_fixture() -> Fixture {
        let message_sender = MessageSender::new_null();
        let send_tracker = message_sender.track();
        let query_tracker = create_query_tracker();

        let clock = SteadyClock::new_null();
        let now = clock.now();
        let query_sender = QuerySender::new_impl(
            message_sender,
            clock,
            Arc::new(Stats::default()),
            query_tracker.clone(),
        );

        Fixture {
            query_sender,
            send_tracker,
            query_tracker,
            now,
        }
    }

    fn create_query_tracker() -> Arc<QueryTracker> {
        Arc::new(QueryTracker::new_null())
    }

    struct Fixture {
        query_sender: QuerySender,
        send_tracker: Arc<OutputTrackerMt<SendEvent>>,
        query_tracker: Arc<QueryTracker>,
        now: Timestamp,
    }
}

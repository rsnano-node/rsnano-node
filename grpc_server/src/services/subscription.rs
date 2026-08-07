use std::collections::HashMap;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::task::{Context, Poll};
use std::time::Duration;

use rsnano_grpc_proto::nano::v1::{
    ConfirmationEvent, ElectionEvent, SubscribeActiveElectionsRequest,
    SubscribeConfirmationsRequest, SubscribeTelemetryRequest, TelemetryEvent,
    subscription_service_server::SubscriptionService,
};
use rsnano_node::Node;
use tokio::runtime::Handle;
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tokio_stream::Stream;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};

type TelemetryItem = Result<TelemetryEvent, Status>;
type TelemetrySubscribers = Arc<Mutex<HashMap<u64, mpsc::Sender<TelemetryItem>>>>;

pub struct SubscriptionServiceImpl {
    max_lag: usize,
    telemetry_dispatcher: Arc<TelemetryDispatcher>,
}

impl SubscriptionServiceImpl {
    pub fn new(node: Arc<Node>, max_lag: usize, send_timeout_ms: u64) -> Self {
        let telemetry_dispatcher = TelemetryDispatcher::new(
            node.runtime.clone(),
            max_lag.max(1),
            Duration::from_millis(send_timeout_ms),
        );
        let publisher = telemetry_dispatcher.publisher();

        node.telemetry
            .on_telemetry_processed(Box::new(move |data, peer_addr| {
                publisher.publish(TelemetryEvent {
                    block_count: data.block_count.to_string(),
                    cemented_count: data.cemented_count.to_string(),
                    unchecked_count: data.unchecked_count.to_string(),
                    account_count: data.account_count.to_string(),
                    bandwidth_cap: data.bandwidth_cap.to_string(),
                    peer_count: data.peer_count.to_string(),
                    protocol_version: data.protocol_version.to_string(),
                    uptime: data.uptime.to_string(),
                    genesis_block: data.genesis_block.to_string(),
                    major_version: data.major_version.to_string(),
                    minor_version: data.minor_version.to_string(),
                    patch_version: data.patch_version.to_string(),
                    pre_release_version: data.pre_release_version.to_string(),
                    maker: data.maker.to_string(),
                    timestamp: data
                        .timestamp
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64,
                    node_id: data.node_id.to_string(),
                    signature: String::new(),
                    address: peer_addr.to_string(),
                });
            }));

        Self {
            max_lag: max_lag.max(1),
            telemetry_dispatcher,
        }
    }
}

#[tonic::async_trait]
impl SubscriptionService for SubscriptionServiceImpl {
    type SubscribeConfirmationsStream =
        Pin<Box<dyn Stream<Item = Result<ConfirmationEvent, Status>> + Send>>;
    type SubscribeTelemetryStream =
        Pin<Box<dyn Stream<Item = Result<TelemetryEvent, Status>> + Send>>;
    type SubscribeActiveElectionsStream =
        Pin<Box<dyn Stream<Item = Result<ElectionEvent, Status>> + Send>>;

    async fn subscribe_confirmations(
        &self,
        request: Request<SubscribeConfirmationsRequest>,
    ) -> Result<Response<Self::SubscribeConfirmationsStream>, Status> {
        let _req = request.into_inner();
        let (_tx, rx) = mpsc::channel::<Result<ConfirmationEvent, Status>>(self.max_lag);

        // TODO: Bridge from NodeEvent::BlockConfirmed to the gRPC stream.
        // This requires registering a handler on the node's CompositeNodeEventHandler
        // during daemon startup and forwarding events to _tx.

        let stream = ReceiverStream::new(rx);
        Ok(Response::new(Box::pin(stream)))
    }

    async fn subscribe_telemetry(
        &self,
        _request: Request<SubscribeTelemetryRequest>,
    ) -> Result<Response<Self::SubscribeTelemetryStream>, Status> {
        Ok(Response::new(Box::pin(
            self.telemetry_dispatcher.subscribe(),
        )))
    }

    async fn subscribe_active_elections(
        &self,
        _request: Request<SubscribeActiveElectionsRequest>,
    ) -> Result<Response<Self::SubscribeActiveElectionsStream>, Status> {
        let (_tx, rx) = mpsc::channel::<Result<ElectionEvent, Status>>(self.max_lag);

        // TODO: Bridge from NodeEvent::ElectionStarted/ElectionStopped to the gRPC stream.
        // This requires registering a handler on the node's CompositeNodeEventHandler
        // during daemon startup and forwarding events to _tx.

        let stream = ReceiverStream::new(rx);
        Ok(Response::new(Box::pin(stream)))
    }
}

struct TelemetryDispatcher {
    ingress: Arc<mpsc::Sender<TelemetryEvent>>,
    subscribers: TelemetrySubscribers,
    next_subscriber_id: AtomicU64,
    subscriber_capacity: usize,
}

impl TelemetryDispatcher {
    fn new(runtime: Handle, capacity: usize, send_timeout: Duration) -> Arc<Self> {
        let (ingress, receiver) = mpsc::channel(capacity);
        let subscribers = Arc::new(Mutex::new(HashMap::new()));
        runtime.spawn(dispatch_telemetry(
            receiver,
            subscribers.clone(),
            send_timeout,
        ));

        Arc::new(Self {
            ingress: Arc::new(ingress),
            subscribers,
            next_subscriber_id: AtomicU64::new(0),
            subscriber_capacity: capacity,
        })
    }

    fn publisher(&self) -> TelemetryPublisher {
        TelemetryPublisher {
            ingress: Arc::downgrade(&self.ingress),
        }
    }

    fn subscribe(&self) -> TelemetryStream {
        let subscriber_id = self.next_subscriber_id.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = mpsc::channel(self.subscriber_capacity);
        self.subscribers
            .lock()
            .unwrap()
            .insert(subscriber_id, sender);

        TelemetryStream {
            inner: ReceiverStream::new(receiver),
            subscriber_id,
            subscribers: Arc::downgrade(&self.subscribers),
        }
    }

    #[cfg(test)]
    fn subscriber_count(&self) -> usize {
        self.subscribers.lock().unwrap().len()
    }
}

#[derive(Clone)]
struct TelemetryPublisher {
    ingress: Weak<mpsc::Sender<TelemetryEvent>>,
}

impl TelemetryPublisher {
    fn publish(&self, event: TelemetryEvent) {
        if let Some(ingress) = self.ingress.upgrade() {
            let _ = ingress.try_send(event);
        }
    }
}

struct TelemetryStream {
    inner: ReceiverStream<TelemetryItem>,
    subscriber_id: u64,
    subscribers: Weak<Mutex<HashMap<u64, mpsc::Sender<TelemetryItem>>>>,
}

impl Stream for TelemetryStream {
    type Item = TelemetryItem;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.get_mut().inner).poll_next(cx)
    }
}

impl Drop for TelemetryStream {
    fn drop(&mut self) {
        if let Some(subscribers) = self.subscribers.upgrade() {
            subscribers.lock().unwrap().remove(&self.subscriber_id);
        }
    }
}

async fn dispatch_telemetry(
    mut receiver: mpsc::Receiver<TelemetryEvent>,
    subscribers: TelemetrySubscribers,
    send_timeout: Duration,
) {
    while let Some(event) = receiver.recv().await {
        let senders: Vec<_> = subscribers
            .lock()
            .unwrap()
            .iter()
            .map(|(id, sender)| (*id, sender.clone()))
            .collect();
        let mut sends = JoinSet::new();

        for (id, sender) in senders {
            let event = event.clone();
            sends.spawn(async move {
                let delivered = matches!(
                    tokio::time::timeout(send_timeout, sender.send(Ok(event))).await,
                    Ok(Ok(()))
                );
                (id, delivered)
            });
        }

        let mut failed = Vec::new();
        while let Some(result) = sends.join_next().await {
            if let Ok((id, false)) = result {
                failed.push(id);
            }
        }

        if !failed.is_empty() {
            subscribers
                .lock()
                .unwrap()
                .retain(|id, _| !failed.contains(id));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_stream::StreamExt;

    #[tokio::test]
    async fn publisher_can_send_from_a_plain_thread() {
        let dispatcher = TelemetryDispatcher::new(Handle::current(), 4, Duration::from_millis(100));
        let publisher = dispatcher.publisher();
        let mut stream = dispatcher.subscribe();

        std::thread::spawn(move || publisher.publish(TelemetryEvent::default()))
            .join()
            .unwrap();

        assert!(stream.next().await.unwrap().is_ok());
    }

    #[tokio::test]
    async fn dropping_stream_unregisters_subscriber() {
        let dispatcher = TelemetryDispatcher::new(Handle::current(), 4, Duration::from_millis(100));
        let stream = dispatcher.subscribe();
        assert_eq!(dispatcher.subscriber_count(), 1);

        drop(stream);

        assert_eq!(dispatcher.subscriber_count(), 0);
    }

    #[tokio::test]
    async fn clients_share_one_dispatcher_registry() {
        let dispatcher = TelemetryDispatcher::new(Handle::current(), 4, Duration::from_millis(100));
        let _first = dispatcher.subscribe();
        let _second = dispatcher.subscribe();

        assert_eq!(dispatcher.subscriber_count(), 2);
    }

    #[tokio::test]
    async fn timed_out_subscriber_is_removed() {
        let dispatcher = TelemetryDispatcher::new(Handle::current(), 1, Duration::from_millis(10));
        let _stream = dispatcher.subscribe();
        let publisher = dispatcher.publisher();
        publisher.publish(TelemetryEvent::default());
        tokio::time::sleep(Duration::from_millis(10)).await;

        publisher.publish(TelemetryEvent::default());
        tokio::time::sleep(Duration::from_millis(20)).await;

        assert_eq!(dispatcher.subscriber_count(), 0);
    }
}

use std::collections::HashMap;
use std::net::SocketAddrV6;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::task::{Context, Poll};
use std::time::{SystemTime, UNIX_EPOCH};

use rsnano_grpc_proto::nano::v1::{
    AccountFilter, BlockHashFilter, BlockProcessCode, BlockProcessingEvent, BlockProcessingResult,
    BlockSource as ProtoBlockSource, ConfirmationEvent, ConfirmationFilter,
    ConfirmationType as ProtoConfirmationType, ElectionEvent, ElectionSummary, ElectionTransition,
    ElectionVote, TelemetryEvent, TelemetryResponse, VoteEvent, WatchBlockProcessingRequest,
    WatchConfirmationsRequest, WatchElectionsRequest, WatchTelemetryRequest, WatchVotesRequest,
    confirmation_filter, event_service_server::EventService,
};
use rsnano_ledger::{BlockError, BlockSource, Ledger, ProcessResult};
use rsnano_messages::TelemetryData;
use rsnano_node::{NodeEvent, NodeEventHandler, consensus::election::ConfirmationType};
use rsnano_types::{Account, BlockHash};
use tokio::sync::mpsc;
use tokio_stream::Stream;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};

use crate::block_conversion::block_record;
use crate::services::block::decode_hash;

type Selector<T> = Arc<dyn Fn(&T) -> Option<T> + Send + Sync>;
type EventItem<T> = Result<T, Status>;

struct Subscriber<T> {
    sender: mpsc::Sender<EventItem<T>>,
    selector: Selector<T>,
    close_status: Arc<Mutex<Option<Status>>>,
}

struct Dispatcher<T> {
    subscribers: Arc<Mutex<HashMap<u64, Subscriber<T>>>>,
    next_id: AtomicU64,
    family: &'static str,
}

impl<T: Clone + Send + 'static> Dispatcher<T> {
    fn new(family: &'static str) -> Self {
        Self {
            subscribers: Arc::new(Mutex::new(HashMap::new())),
            next_id: AtomicU64::new(0),
            family,
        }
    }

    fn subscribe(&self, capacity: usize, selector: Selector<T>) -> EventStream<T> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = mpsc::channel(capacity.max(1));
        let close_status = Arc::new(Mutex::new(None));
        lock(&self.subscribers).insert(
            id,
            Subscriber {
                sender,
                selector,
                close_status: close_status.clone(),
            },
        );
        EventStream {
            inner: ReceiverStream::new(receiver),
            id,
            subscribers: Arc::downgrade(&self.subscribers),
            close_status,
            emitted_close_status: false,
        }
    }

    fn publish(&self, event: T) {
        lock(&self.subscribers).retain(|_, subscriber| {
            let Some(selected) = (subscriber.selector)(&event) else {
                return true;
            };
            match subscriber.sender.try_send(Ok(selected)) {
                Ok(()) => true,
                Err(mpsc::error::TrySendError::Full(_)) => {
                    *lock(&subscriber.close_status) = Some(Status::resource_exhausted(format!(
                        "{} subscriber queue is full",
                        self.family
                    )));
                    false
                }
                Err(mpsc::error::TrySendError::Closed(_)) => false,
            }
        });
    }

    #[cfg(test)]
    fn subscriber_count(&self) -> usize {
        lock(&self.subscribers).len()
    }
}

pub struct GrpcEventHub {
    confirmations: Dispatcher<ConfirmationEvent>,
    processing: Dispatcher<BlockProcessingEvent>,
    elections: Dispatcher<ElectionEvent>,
    votes: Dispatcher<VoteEvent>,
    telemetry: Dispatcher<TelemetryEvent>,
}

impl GrpcEventHub {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            confirmations: Dispatcher::new("confirmation"),
            processing: Dispatcher::new("block processing"),
            elections: Dispatcher::new("election"),
            votes: Dispatcher::new("vote"),
            telemetry: Dispatcher::new("telemetry"),
        })
    }

    pub fn publish_telemetry(&self, data: &TelemetryData, endpoint: &SocketAddrV6) {
        self.telemetry.publish(TelemetryEvent {
            endpoint: endpoint.to_string(),
            telemetry: Some(telemetry_response(data)),
            observed_at_unix_ms: now_ms(),
        });
    }
}

pub struct GrpcNodeEventHandler {
    hub: Arc<GrpcEventHub>,
    ledger: Arc<Ledger>,
}

impl GrpcNodeEventHandler {
    pub fn new(hub: Arc<GrpcEventHub>, ledger: Arc<Ledger>) -> Self {
        Self { hub, ledger }
    }
}

impl NodeEventHandler for GrpcNodeEventHandler {
    fn handle(&mut self, event: &NodeEvent) {
        match event {
            NodeEvent::BlockConfirmed(block, election) => {
                let votes = election
                    .votes
                    .values()
                    .map(|vote| ElectionVote {
                        representative: vote.voter.as_account().encode_account(),
                        block_hash: vote.hash.to_string(),
                        weight_raw: vote.weight.to_string_dec(),
                        created_timestamp_ms: vote.vote_created.as_u64(),
                        final_vote: vote.is_final_vote(),
                    })
                    .collect();
                self.hub.confirmations.publish(ConfirmationEvent {
                    block: Some(block_record(&self.ledger, block)),
                    confirmation_type: confirmation_type(election.confirmation_type) as i32,
                    observed_at_unix_ms: now_ms(),
                    election: Some(ElectionSummary {
                        tally_raw: election.tally.to_string_dec(),
                        final_tally_raw: election.final_tally.to_string_dec(),
                        candidate_block_count: election.block_count,
                        voter_count: election.voter_count,
                        duration_ms: election.election_duration.as_millis() as u64,
                        votes,
                    }),
                });
            }
            NodeEvent::BlocksProcessed(results) => {
                self.hub
                    .processing
                    .publish(processing_event(&self.ledger, results));
            }
            NodeEvent::ElectionStarted(hash) => self.hub.elections.publish(ElectionEvent {
                block_hash: hash.to_string(),
                transition: ElectionTransition::Started as i32,
                observed_at_unix_ms: now_ms(),
            }),
            NodeEvent::ElectionStopped(hash) => self.hub.elections.publish(ElectionEvent {
                block_hash: hash.to_string(),
                transition: ElectionTransition::Stopped as i32,
                observed_at_unix_ms: now_ms(),
            }),
            NodeEvent::VoteProcessed(vote, result) => self.hub.votes.publish(VoteEvent {
                representative: vote.voter.as_account().encode_account(),
                signature: vote.signature.encode_hex(),
                block_hashes: vote.hashes.iter().map(ToString::to_string).collect(),
                created_timestamp_ms: vote.timestamp().as_u64(),
                final_vote: vote.is_final(),
                result: match result {
                    Ok(()) => "vote".to_string(),
                    Err(error) => error.as_str().to_string(),
                },
                observed_at_unix_ms: now_ms(),
            }),
        }
    }
}

pub struct EventServiceImpl {
    hub: Arc<GrpcEventHub>,
    max_lag: usize,
}

impl EventServiceImpl {
    pub fn new(hub: Arc<GrpcEventHub>, max_lag: usize) -> Self {
        Self {
            hub,
            max_lag: max_lag.max(1),
        }
    }
}

type BoxEventStream<T> = Pin<Box<dyn Stream<Item = EventItem<T>> + Send>>;

#[tonic::async_trait]
impl EventService for EventServiceImpl {
    type WatchConfirmationsStream = BoxEventStream<ConfirmationEvent>;
    type WatchBlockProcessingStream = BoxEventStream<BlockProcessingEvent>;
    type WatchElectionsStream = BoxEventStream<ElectionEvent>;
    type WatchVotesStream = BoxEventStream<VoteEvent>;
    type WatchTelemetryStream = BoxEventStream<TelemetryEvent>;

    async fn watch_confirmations(
        &self,
        request: Request<WatchConfirmationsRequest>,
    ) -> Result<Response<Self::WatchConfirmationsStream>, Status> {
        let request = request.into_inner();
        let filter = ConfirmationMatcher::parse(request.filter)?;
        let types = request
            .confirmation_types
            .into_iter()
            .map(|value| {
                ProtoConfirmationType::try_from(value)
                    .map_err(|_| Status::invalid_argument("invalid confirmation type"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let include_votes = request.include_election_votes;
        let selector = Arc::new(move |event: &ConfirmationEvent| {
            if !types.is_empty()
                && !types
                    .iter()
                    .any(|value| *value as i32 == event.confirmation_type)
            {
                return None;
            }
            if !filter.matches(event) {
                return None;
            }
            let mut event = event.clone();
            if !include_votes {
                if let Some(election) = &mut event.election {
                    election.votes.clear();
                }
            }
            Some(event)
        });
        Ok(Response::new(Box::pin(
            self.hub.confirmations.subscribe(self.max_lag, selector),
        )))
    }

    async fn watch_block_processing(
        &self,
        _: Request<WatchBlockProcessingRequest>,
    ) -> Result<Response<Self::WatchBlockProcessingStream>, Status> {
        Ok(Response::new(Box::pin(
            self.hub.processing.subscribe(self.max_lag, identity()),
        )))
    }
    async fn watch_elections(
        &self,
        _: Request<WatchElectionsRequest>,
    ) -> Result<Response<Self::WatchElectionsStream>, Status> {
        Ok(Response::new(Box::pin(
            self.hub.elections.subscribe(self.max_lag, identity()),
        )))
    }
    async fn watch_votes(
        &self,
        _: Request<WatchVotesRequest>,
    ) -> Result<Response<Self::WatchVotesStream>, Status> {
        Ok(Response::new(Box::pin(
            self.hub.votes.subscribe(self.max_lag, identity()),
        )))
    }
    async fn watch_telemetry(
        &self,
        _: Request<WatchTelemetryRequest>,
    ) -> Result<Response<Self::WatchTelemetryStream>, Status> {
        Ok(Response::new(Box::pin(
            self.hub.telemetry.subscribe(self.max_lag, identity()),
        )))
    }
}

enum ConfirmationMatcher {
    All,
    Hashes(Vec<BlockHash>),
    Accounts(Vec<Account>),
}

impl ConfirmationMatcher {
    fn parse(filter: Option<ConfirmationFilter>) -> Result<Self, Status> {
        match filter.and_then(|value| value.mode) {
            None | Some(confirmation_filter::Mode::All(true)) => Ok(Self::All),
            Some(confirmation_filter::Mode::All(false)) => {
                Err(Status::invalid_argument("all must be true"))
            }
            Some(confirmation_filter::Mode::BlockHashes(BlockHashFilter { block_hashes })) => {
                if block_hashes.is_empty() {
                    return Err(Status::invalid_argument("block hash filter is empty"));
                }
                Ok(Self::Hashes(
                    block_hashes
                        .iter()
                        .map(|hash| decode_hash(hash))
                        .collect::<Result<_, _>>()?,
                ))
            }
            Some(confirmation_filter::Mode::Accounts(AccountFilter { accounts })) => {
                if accounts.is_empty() {
                    return Err(Status::invalid_argument("account filter is empty"));
                }
                Ok(Self::Accounts(
                    accounts
                        .iter()
                        .map(|value| {
                            Account::parse(value)
                                .ok_or_else(|| Status::invalid_argument("invalid account filter"))
                        })
                        .collect::<Result<_, _>>()?,
                ))
            }
        }
    }

    fn matches(&self, event: &ConfirmationEvent) -> bool {
        let Some(block) = &event.block else {
            return false;
        };
        match self {
            Self::All => true,
            Self::Hashes(hashes) => {
                BlockHash::decode_hex(&block.block_hash).is_some_and(|hash| hashes.contains(&hash))
            }
            Self::Accounts(accounts) => {
                let owner = block
                    .sideband
                    .as_ref()
                    .and_then(|sideband| Account::parse(&sideband.account));
                let linked = block.linked_account.as_deref().and_then(Account::parse);
                accounts
                    .iter()
                    .any(|account| owner == Some(*account) || linked == Some(*account))
            }
        }
    }
}

fn processing_event(ledger: &Ledger, results: &[ProcessResult]) -> BlockProcessingEvent {
    BlockProcessingEvent {
        observed_at_unix_ms: now_ms(),
        results: results
            .iter()
            .map(|result| BlockProcessingResult {
                block_hash: result.block.hash().to_string(),
                code: process_code(&result.status) as i32,
                block: result
                    .saved_block
                    .as_ref()
                    .map(|block| block_record(ledger, block)),
                source: proto_source(result.source) as i32,
            })
            .collect(),
    }
}

fn process_code(status: &Result<(), BlockError>) -> BlockProcessCode {
    match status {
        Ok(()) => BlockProcessCode::Accepted,
        Err(BlockError::Old(_)) => BlockProcessCode::Old,
        Err(BlockError::BadSignature) => BlockProcessCode::BadSignature,
        Err(BlockError::NegativeSpend) => BlockProcessCode::NegativeSpend,
        Err(BlockError::Fork) => BlockProcessCode::Fork,
        Err(BlockError::Unreceivable) => BlockProcessCode::Unreceivable,
        Err(BlockError::GapPrevious) => BlockProcessCode::GapPrevious,
        Err(BlockError::GapSource) => BlockProcessCode::GapSource,
        Err(BlockError::OpenedBurnAccount) => BlockProcessCode::OpenedBurnAccount,
        Err(BlockError::BalanceMismatch) => BlockProcessCode::BalanceMismatch,
        Err(BlockError::RepresentativeMismatch) => BlockProcessCode::RepresentativeMismatch,
        Err(BlockError::BlockPosition) => BlockProcessCode::BlockPosition,
        Err(BlockError::InsufficientWork) => BlockProcessCode::InsufficientWork,
        Err(BlockError::GapEpochOpenPending) => BlockProcessCode::GapEpochOpenPending,
        Err(BlockError::Conflict) => BlockProcessCode::Conflict,
    }
}

fn confirmation_type(value: ConfirmationType) -> ProtoConfirmationType {
    match value {
        ConfirmationType::ActiveConfirmedQuorum => ProtoConfirmationType::ActiveQuorum,
        ConfirmationType::ActiveConfirmationHeight => {
            ProtoConfirmationType::ActiveConfirmationHeight
        }
        ConfirmationType::InactiveConfirmationHeight => ProtoConfirmationType::Inactive,
    }
}

fn proto_source(value: BlockSource) -> ProtoBlockSource {
    match value {
        BlockSource::Live => ProtoBlockSource::Live,
        BlockSource::LiveOriginator => ProtoBlockSource::LiveOriginator,
        BlockSource::Bootstrap => ProtoBlockSource::Bootstrap,
        BlockSource::Unchecked => ProtoBlockSource::Unchecked,
        BlockSource::Local => ProtoBlockSource::Local,
        BlockSource::Forced => ProtoBlockSource::Forced,
    }
}

fn telemetry_response(data: &TelemetryData) -> TelemetryResponse {
    TelemetryResponse {
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
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
        node_id: data.node_id.to_string(),
        signature: data.signature.encode_hex(),
    }
}

fn identity<T: Clone + Send + Sync + 'static>() -> Selector<T> {
    Arc::new(|event| Some(event.clone()))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

struct EventStream<T> {
    inner: ReceiverStream<EventItem<T>>,
    id: u64,
    subscribers: Weak<Mutex<HashMap<u64, Subscriber<T>>>>,
    close_status: Arc<Mutex<Option<Status>>>,
    emitted_close_status: bool,
}

impl<T> Stream for EventStream<T> {
    type Item = EventItem<T>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::new(&mut self.inner).poll_next(cx) {
            Poll::Ready(None) if !self.emitted_close_status => {
                self.emitted_close_status = true;
                let status = lock(&self.close_status).take();
                status.map_or(Poll::Ready(None), |status| Poll::Ready(Some(Err(status))))
            }
            result => result,
        }
    }
}

impl<T> Drop for EventStream<T> {
    fn drop(&mut self) {
        if let Some(subscribers) = self.subscribers.upgrade() {
            lock(&subscribers).remove(&self.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsnano_grpc_proto::nano::v1::{BlockRecord, BlockSideband};
    use tokio_stream::StreamExt;

    #[tokio::test]
    async fn cancellation_unregisters_only_its_dispatcher() {
        let hub = GrpcEventHub::new();
        let stream = hub.elections.subscribe(1, identity());
        assert_eq!(hub.elections.subscriber_count(), 1);
        assert_eq!(hub.votes.subscriber_count(), 0);
        drop(stream);
        assert_eq!(hub.elections.subscriber_count(), 0);
    }

    #[tokio::test]
    async fn slow_subscriber_receives_resource_exhausted_without_affecting_other_families() {
        let hub = GrpcEventHub::new();
        let mut slow = hub.elections.subscribe(1, identity());
        let mut votes = hub.votes.subscribe(1, identity());
        let election = ElectionEvent::default();
        hub.elections.publish(election.clone());
        hub.elections.publish(election);
        hub.votes.publish(VoteEvent::default());
        assert!(slow.next().await.unwrap().is_ok());
        assert_eq!(
            slow.next().await.unwrap().unwrap_err().code(),
            tonic::Code::ResourceExhausted
        );
        assert!(votes.next().await.unwrap().is_ok());
    }

    #[test]
    fn duplicate_notifications_are_not_hidden() {
        let hub = GrpcEventHub::new();
        let mut stream = hub.elections.subscribe(2, identity());
        let event = ElectionEvent {
            block_hash: BlockHash::from(1).to_string(),
            ..Default::default()
        };
        hub.elections.publish(event.clone());
        hub.elections.publish(event);
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            assert_eq!(
                stream.next().await.unwrap().unwrap().block_hash,
                stream.next().await.unwrap().unwrap().block_hash
            );
        });
    }

    #[test]
    fn account_filter_matches_chain_owner_or_ledger_linked_account() {
        let owner = Account::from(1);
        let linked = Account::from(2);
        let event = ConfirmationEvent {
            block: Some(BlockRecord {
                sideband: Some(BlockSideband {
                    account: owner.encode_account(),
                    ..Default::default()
                }),
                linked_account: Some(linked.encode_account()),
                ..Default::default()
            }),
            ..Default::default()
        };

        assert!(ConfirmationMatcher::Accounts(vec![owner]).matches(&event));
        assert!(ConfirmationMatcher::Accounts(vec![linked]).matches(&event));
        assert!(!ConfirmationMatcher::Accounts(vec![Account::from(3)]).matches(&event));
    }
}

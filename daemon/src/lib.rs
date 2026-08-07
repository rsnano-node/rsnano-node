mod http_callbacks;

#[cfg(feature = "grpc")]
use anyhow::Context;
use file_mode::set_umask;
use http_callbacks::HttpCallbacks;
use rsnano_node::{
    CompositeNodeEventHandler, Node, NodeBuilder, NodeCallbacks, NodeEvent, NodeEventHandler,
    config::{DaemonConfig, NetworkType, NodeFlags},
};
use rsnano_rpc_server::{RpcServerConfig, run_rpc_server};
use rsnano_utils::get_cpu_count;
use rsnano_websocket_server::{WebsocketListenerExt, create_websocket_server};
use std::{
    future::Future,
    path::PathBuf,
    sync::{Arc, mpsc::sync_channel},
    thread::available_parallelism,
};
use tokio::{net::TcpListener, sync::oneshot};
use tracing::{info, warn};

pub struct DaemonBuilder {
    network: NetworkType,
    node_builder: NodeBuilder,
    node_started: Option<Box<dyn FnMut(Arc<Node>) + Send>>,
    event_handler: Option<Box<dyn FnMut(&NodeEvent) + Send>>,
}

impl DaemonBuilder {
    pub fn new(network: NetworkType) -> Self {
        Self {
            network,
            node_builder: NodeBuilder::new(network),
            node_started: None,
            event_handler: None,
        }
    }

    pub fn data_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.node_builder = self.node_builder.data_path(path);
        self
    }

    pub fn flags(mut self, flags: NodeFlags) -> Self {
        self.node_builder = self.node_builder.flags(flags);
        self
    }

    pub fn callbacks(mut self, callbacks: NodeCallbacks) -> Self {
        self.node_builder = self.node_builder.callbacks(callbacks);
        self
    }

    pub fn on_node_started(mut self, callback: impl FnMut(Arc<Node>) + Send + 'static) -> Self {
        self.node_started = Some(Box::new(callback));
        self
    }

    pub fn on_node_event(mut self, callback: impl FnMut(&NodeEvent) + Send + 'static) -> Self {
        self.event_handler = Some(Box::new(callback));
        self
    }

    pub fn run<F>(mut self, shutdown: F) -> anyhow::Result<()>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        set_umask(0o077);
        set_max_file_descriptor_limit()?;

        let data_path = self.node_builder.get_data_path()?;
        let parallelism = get_cpu_count();
        let daemon_config =
            DaemonConfig::load_from_data_path(self.network, parallelism, &data_path)?;
        let rpc_config = RpcServerConfig::load_from_data_path(self.network, &data_path)?;

        info!("Starting up RsNano node...");

        info!(
            "Hardware concurrency: {} (configured: {})",
            available_parallelism().unwrap().get(),
            parallelism
        );

        log_file_descriptor_limit()?;

        let websocket_enabled = daemon_config.node.websocket_config.enabled;
        let http_callback_enabled = daemon_config.node.rpc_callback_url().is_some();
        let mut websocket_server = None;
        let mut node;

        if websocket_enabled || http_callback_enabled || self.event_handler.is_some() {
            let (ev_sender, ev_receiver) = sync_channel(1024 * 16);
            node = self.node_builder.event_sink(ev_sender).finish()?;
            let mut event_processor = CompositeNodeEventHandler::new(ev_receiver);

            websocket_server = if websocket_enabled {
                Some(
                    create_websocket_server(
                        daemon_config.node.websocket_config.clone(),
                        &node,
                        &mut event_processor,
                    )
                    .unwrap(),
                )
            } else {
                None
            };

            if let Some(ref websocket) = websocket_server {
                websocket.start();
            }

            if let Some(callback_url) = daemon_config.node.rpc_callback_url() {
                info!("HTTP callbacks enabled on {:?}", callback_url);
                let http_callbacks = HttpCallbacks {
                    runtime: node.runtime.clone(),
                    stats: node.stats.clone(),
                    ledger: node.ledger.clone(),
                    callback_url,
                };
                event_processor.add(http_callbacks);
            }

            if let Some(event_handler) = self.event_handler.take() {
                event_processor.add(ForwardNodeEvent(event_handler))
            }

            std::thread::Builder::new()
                .name("Node ev proc".to_owned())
                .spawn(move || {
                    event_processor.run();
                })
                .unwrap();
        } else {
            node = self.node_builder.finish()?;
        }

        node.start();
        let mut node = Arc::new(node);

        if let Some(mut started_callback) = self.node_started {
            started_callback(node.clone());
        }
        let (tx_stop, rx_stop) = oneshot::channel();
        let wait_for_shutdown = async move {
            tokio::select! {
                _ = rx_stop =>{}
                _ = shutdown => {}
            }
        };

        node.runtime.block_on(run_services(
            daemon_config,
            rpc_config,
            node.clone(),
            tx_stop,
            wait_for_shutdown,
        ))?;

        if let Some(ref websocket) = websocket_server {
            websocket.stop();
        }

        let node = Arc::get_mut(&mut node).expect("No exclusive access to node!");
        node.stop();
        Ok(())
    }
}

fn set_max_file_descriptor_limit() -> std::io::Result<()> {
    let (_, hard) = rlimit::getrlimit(rlimit::Resource::NOFILE)?;
    rlimit::setrlimit(rlimit::Resource::NOFILE, hard, hard)
}

fn log_file_descriptor_limit() -> Result<(), std::io::Error> {
    let (soft, hard) = rlimit::getrlimit(rlimit::Resource::NOFILE)?;
    info!(soft, hard, "File descriptor limit");
    const RECOMMENDED_FILE_DESC_LIMIT: u64 = 65535;
    Ok(if soft < RECOMMENDED_FILE_DESC_LIMIT {
        warn!(
            "Current file descriptor limit of {} is lower than the {} recommended. Node was unable to change it.",
            soft, RECOMMENDED_FILE_DESC_LIMIT
        );
    })
}

struct ForwardNodeEvent(Box<dyn FnMut(&NodeEvent) + Send>);

impl NodeEventHandler for ForwardNodeEvent {
    fn handle(&mut self, event: &NodeEvent) {
        (self.0)(event);
    }
}

async fn run_services(
    daemon_config: DaemonConfig,
    rpc_config: RpcServerConfig,
    node: Arc<Node>,
    tx_stop: oneshot::Sender<()>,
    wait_for_shutdown: impl Future<Output = ()> + Send + 'static,
) -> anyhow::Result<()> {
    #[cfg(feature = "grpc")]
    {
        let grpc_config = rsnano_grpc_server::GrpcServerConfig::load_from_data_path(
            node.network_params.network.current_network,
            &node.data_path,
        )?;
        let grpc_shutdown = tokio_util::sync::CancellationToken::new();
        let grpc_shutdown_token = grpc_shutdown.clone();

        let node_clone = node.clone();
        let grpc_handle = node.runtime.spawn(async move {
            rsnano_grpc_server::run_grpc_server(
                node_clone,
                grpc_config,
                grpc_shutdown_token.cancelled(),
            )
            .await
        });

        monitor_grpc(
            run_rpc(daemon_config, rpc_config, node, tx_stop, wait_for_shutdown),
            grpc_handle,
            grpc_shutdown,
        )
        .await?;
    }

    #[cfg(not(feature = "grpc"))]
    {
        run_rpc(daemon_config, rpc_config, node, tx_stop, wait_for_shutdown).await?;
    }

    Ok(())
}

#[cfg(feature = "grpc")]
async fn monitor_grpc(
    rpc: impl Future<Output = anyhow::Result<()>>,
    mut grpc_handle: tokio::task::JoinHandle<anyhow::Result<()>>,
    grpc_shutdown: tokio_util::sync::CancellationToken,
) -> anyhow::Result<()> {
    tokio::select! {
        rpc_result = rpc => {
            grpc_shutdown.cancel();
            grpc_handle
                .await
                .map_err(|error| anyhow::anyhow!("gRPC server task failed: {error}"))?
                .context("gRPC server shutdown failed")?;
            rpc_result
        }
        grpc_result = &mut grpc_handle => {
            grpc_shutdown.cancel();
            match grpc_result
                .map_err(|error| anyhow::anyhow!("gRPC server task failed: {error}"))?
            {
                Ok(()) => anyhow::bail!("gRPC server stopped unexpectedly"),
                Err(error) => Err(error.context("gRPC server failed")),
            }
        }
    }
}

async fn run_rpc(
    daemon_config: DaemonConfig,
    rpc_config: RpcServerConfig,
    node: Arc<Node>,
    tx_stop: oneshot::Sender<()>,
    wait_for_shutdown: impl Future<Output = ()> + Send + 'static,
) -> anyhow::Result<()> {
    if daemon_config.rpc_enable {
        let socket_addr = rpc_config.listening_addr()?;
        let listener = TcpListener::bind(socket_addr).await?;
        run_rpc_server(
            node.clone(),
            listener,
            rpc_config.enable_control,
            tx_stop,
            wait_for_shutdown,
        )
        .await?;
    } else {
        wait_for_shutdown.await;
    };
    Ok(())
}

#[cfg(all(test, feature = "grpc"))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn grpc_server_error_is_propagated_while_rpc_is_running() {
        let grpc_handle = tokio::spawn(async { Err(anyhow::anyhow!("address already in use")) });
        let rpc = std::future::pending::<anyhow::Result<()>>();

        let error = monitor_grpc(rpc, grpc_handle, tokio_util::sync::CancellationToken::new())
            .await
            .unwrap_err();

        assert_eq!(
            format!("{error:#}"),
            "gRPC server failed: address already in use"
        );
    }

    #[tokio::test]
    async fn grpc_server_is_cancelled_when_rpc_stops() {
        let shutdown = tokio_util::sync::CancellationToken::new();
        let wait_for_shutdown = shutdown.clone();
        let grpc_handle = tokio::spawn(async move {
            wait_for_shutdown.cancelled().await;
            Ok(())
        });

        let result = monitor_grpc(async { Ok(()) }, grpc_handle, shutdown).await;

        assert!(result.is_ok(), "monitoring failed: {result:?}");
    }

    #[tokio::test]
    async fn unsolicited_grpc_server_stop_is_an_error() {
        let grpc_handle = tokio::spawn(async { Ok(()) });
        let rpc = std::future::pending::<anyhow::Result<()>>();

        let error = monitor_grpc(rpc, grpc_handle, tokio_util::sync::CancellationToken::new())
            .await
            .unwrap_err();

        assert_eq!(error.to_string(), "gRPC server stopped unexpectedly");
    }
}

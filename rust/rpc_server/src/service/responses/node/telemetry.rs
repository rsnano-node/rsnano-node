use rsnano_node::Node;
use rsnano_rpc_messages::{ErrorDto, TelemetryDto, TelemetryDtos};
use serde_json::to_string_pretty;
use std::{
    net::{Ipv6Addr, SocketAddrV6},
    sync::Arc,
};

pub async fn telemetry(
    node: Arc<Node>,
    address: Option<Ipv6Addr>,
    port: Option<u16>,
    raw: Option<bool>,
) -> String {
    if let (Some(address), Some(port)) = (address, port) {
        let endpoint = SocketAddrV6::new(address, port, 0, 0);

        if address.is_loopback() && port == node.network.port() {
            to_string_pretty(&TelemetryDtos {
                metrics: vec![node.telemetry.local_telemetry().into()],
            })
            .unwrap()
        } else {
            match node.telemetry.get_telemetry(&endpoint.into()) {
                Some(data) => to_string_pretty(&TelemetryDtos {
                    metrics: vec![data.into()],
                })
                .unwrap(),
                None => to_string_pretty(&ErrorDto::new("Peer not found".to_string())).unwrap(),
            }
        }
    } else if address.is_some() || port.is_some() {
        to_string_pretty(&ErrorDto::new(
            "Both address and port are required".to_string(),
        ))
        .unwrap()
    } else {
        let output_raw = raw.unwrap_or(false);

        if output_raw {
            let all_telemetries = node.telemetry.get_all_telemetries();
            let metrics: Vec<TelemetryDto> = all_telemetries
                .iter()
                .map(|(endpoint, telemetry)| {
                    let mut dto: TelemetryDto = telemetry.clone().into();
                    dto.address = Some(*endpoint.ip());
                    dto.port = Some(endpoint.port());
                    dto
                })
                .collect();

            to_string_pretty(&TelemetryDtos { metrics }).unwrap()
        } else {
            to_string_pretty(&TelemetryDtos {
                metrics: vec![node.telemetry.local_telemetry().into()],
            })
            .unwrap()
        }
    }
}

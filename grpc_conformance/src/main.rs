use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use reqwest::Client;
use rsnano_grpc_conformance::{Report, ScenarioResult};
use rsnano_grpc_proto::nano::v1::account_service_client::AccountServiceClient;
use rsnano_grpc_proto::nano::v1::block_service_client::BlockServiceClient;
use rsnano_grpc_proto::nano::v1::ledger_service_client::LedgerServiceClient;
use rsnano_grpc_proto::nano::v1::network_service_client::NetworkServiceClient;
use rsnano_grpc_proto::nano::v1::node_service_client::NodeServiceClient;
use rsnano_grpc_proto::nano::v1::{
    AccountBalanceRequest, AccountHistoryRequest, AccountInfoRequest, AccountRepresentativeRequest,
    BlockInfoRequest, BlocksRequest, FrontierCountRequest, PeersRequest, ProcessRequest,
    ReceivableBlocksRequest, StatusRequest, TelemetryRequest, VersionRequest,
};
use rsnano_ledger::{DEV_GENESIS_ACCOUNT, DEV_GENESIS_HASH};
use serde_json::{Value, json};
use tonic::transport::{Channel, Endpoint};
use tonic::{Code, Status};

const ACCOUNT_INFO: &str = "AccountService.AccountInfo";
const ACCOUNT_BALANCE: &str = "AccountService.AccountBalance";
const ACCOUNT_HISTORY: &str = "AccountService.AccountHistory";
const ACCOUNT_REPRESENTATIVE: &str = "AccountService.AccountRepresentative";
const PROCESS: &str = "BlockService.Process";
const BLOCK_INFO: &str = "BlockService.BlockInfo";
const BLOCKS: &str = "BlockService.Blocks";
const FRONTIER_COUNT: &str = "LedgerService.FrontierCount";
const RECEIVABLE_BLOCKS: &str = "LedgerService.ReceivableBlocks";
const PEERS: &str = "NetworkService.Peers";
const TELEMETRY: &str = "NetworkService.Telemetry";
const STATUS: &str = "NodeService.Status";
const VERSION: &str = "NodeService.Version";

const MISSING_ACCOUNT: &str = "nano_3t6k35gi95xu6tergt6p69ck76ogmitsa8mnijtpxm9fkcm736xtoncuohr3";
const INVALID_ACCOUNT: &str = "not-a-nano-account";
const MISSING_HASH: &str = "1111111111111111111111111111111111111111111111111111111111111111";

#[derive(Debug)]
struct Config {
    rpc_url: String,
    grpc_url: String,
    report_path: PathBuf,
}

impl Config {
    fn from_args() -> Result<Self, String> {
        let mut rpc_url = "http://[::1]:45000".to_string();
        let mut grpc_url = "http://[::1]:47078".to_string();
        let mut report_path = PathBuf::from("grpc_conformance/reports/current.md");
        let mut args = env::args().skip(1);

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--rpc-url" => rpc_url = next_value(&mut args, "--rpc-url")?,
                "--grpc-url" => grpc_url = next_value(&mut args, "--grpc-url")?,
                "--report" => report_path = next_value(&mut args, "--report")?.into(),
                "--help" | "-h" => {
                    println!("grpc-conformance [--rpc-url URL] [--grpc-url URL] [--report PATH]");
                    return Err(String::new());
                }
                _ => return Err(format!("unknown argument: {arg}")),
            }
        }

        Ok(Self {
            rpc_url,
            grpc_url,
            report_path,
        })
    }
}

fn next_value(args: &mut impl Iterator<Item = String>, option: &str) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("{option} requires a value"))
}

#[derive(Clone)]
struct RpcClient {
    url: String,
    client: Client,
}

impl RpcClient {
    fn new(url: String) -> Result<Self, String> {
        let client = Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .map_err(|error| format!("create JSON-RPC client: {error}"))?;
        Ok(Self { url, client })
    }

    async fn call(&self, request: Value) -> Result<Value, String> {
        let response = self
            .client
            .post(&self.url)
            .json(&request)
            .send()
            .await
            .map_err(|error| format!("JSON-RPC request failed: {error}"))?;
        let status = response.status();
        let body = response
            .json::<Value>()
            .await
            .map_err(|error| format!("decode JSON-RPC response: {error}"))?;

        if status.is_success() {
            Ok(body)
        } else {
            Err(format!("JSON-RPC returned HTTP {status}: {body}"))
        }
    }
}

struct Harness {
    rpc: RpcClient,
    account: AccountServiceClient<Channel>,
    block: BlockServiceClient<Channel>,
    ledger: LedgerServiceClient<Channel>,
    network: NetworkServiceClient<Channel>,
    node: NodeServiceClient<Channel>,
    genesis_account: String,
    genesis_hash: String,
}

impl Harness {
    async fn connect(config: &Config) -> Result<Self, String> {
        let rpc = RpcClient::new(config.rpc_url.clone())?;
        let endpoint = Endpoint::from_shared(config.grpc_url.clone())
            .map_err(|error| format!("invalid gRPC URL: {error}"))?
            .connect_timeout(Duration::from_secs(2));

        let mut last_error = "services did not become ready".to_string();
        for _ in 0..60 {
            let rpc_ready = rpc.call(json!({ "action": "version" })).await;
            match (rpc_ready, endpoint.connect().await) {
                (Ok(_), Ok(channel)) => {
                    return Ok(Self {
                        rpc,
                        account: AccountServiceClient::new(channel.clone()),
                        block: BlockServiceClient::new(channel.clone()),
                        ledger: LedgerServiceClient::new(channel.clone()),
                        network: NetworkServiceClient::new(channel.clone()),
                        node: NodeServiceClient::new(channel),
                        genesis_account: DEV_GENESIS_ACCOUNT.encode_account(),
                        genesis_hash: DEV_GENESIS_HASH.to_string(),
                    });
                }
                (rpc_result, grpc_result) => {
                    last_error = format!(
                        "JSON-RPC: {}; gRPC: {}",
                        display_result(rpc_result),
                        display_result(grpc_result)
                    );
                }
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        Err(format!(
            "services were not ready after 30 seconds: {last_error}"
        ))
    }

    async fn account_info_genesis(&mut self) -> Result<(), String> {
        let json = self
            .rpc
            .call(json!({
                "action": "account_info",
                "account": self.genesis_account
            }))
            .await?;
        let grpc = self
            .account
            .account_info(AccountInfoRequest {
                account: self.genesis_account.clone(),
                receivable: false,
                representative: false,
                weight: false,
                pending: false,
            })
            .await
            .map_err(display_status)?
            .into_inner();

        compare("frontier", &grpc.frontier, json_string(&json, "frontier")?)?;
        compare(
            "open_block",
            &grpc.open_block,
            json_string(&json, "open_block")?,
        )?;
        compare(
            "representative_block",
            &grpc.representative_block,
            json_string(&json, "representative_block")?,
        )?;
        compare("balance", &grpc.balance, json_string(&json, "balance")?)?;
        compare(
            "block_count",
            &grpc.block_count.to_string(),
            json_string(&json, "block_count")?,
        )?;
        compare(
            "confirmation_height",
            &grpc.confirmation_height,
            json_string(&json, "confirmation_height")?,
        )?;
        Ok(())
    }

    async fn account_info_invalid(&mut self) -> Result<(), String> {
        expect_code(
            self.account
                .account_info(AccountInfoRequest {
                    account: INVALID_ACCOUNT.to_string(),
                    receivable: false,
                    representative: false,
                    weight: false,
                    pending: false,
                })
                .await,
            Code::InvalidArgument,
        )
    }

    async fn account_info_missing(&mut self) -> Result<(), String> {
        expect_code(
            self.account
                .account_info(AccountInfoRequest {
                    account: MISSING_ACCOUNT.to_string(),
                    receivable: false,
                    representative: false,
                    weight: false,
                    pending: false,
                })
                .await,
            Code::NotFound,
        )
    }

    async fn account_balance_genesis(&mut self) -> Result<(), String> {
        let json = self
            .rpc
            .call(json!({
                "action": "account_balance",
                "account": self.genesis_account
            }))
            .await?;
        let grpc = self
            .account
            .account_balance(AccountBalanceRequest {
                account: self.genesis_account.clone(),
            })
            .await
            .map_err(display_status)?
            .into_inner();

        compare("balance", &grpc.balance, json_string(&json, "balance")?)?;
        compare(
            "receivable",
            &grpc.receivable,
            json_string(&json, "receivable")?,
        )
    }

    async fn account_balance_unopened(&mut self) -> Result<(), String> {
        let json = self
            .rpc
            .call(json!({
                "action": "account_balance",
                "account": MISSING_ACCOUNT
            }))
            .await?;
        let grpc = self
            .account
            .account_balance(AccountBalanceRequest {
                account: MISSING_ACCOUNT.to_string(),
            })
            .await
            .map_err(display_status)?
            .into_inner();

        compare("balance", &grpc.balance, json_string(&json, "balance")?)?;
        compare(
            "receivable",
            &grpc.receivable,
            json_string(&json, "receivable")?,
        )
    }

    async fn account_history_genesis(&mut self) -> Result<(), String> {
        let json = self
            .rpc
            .call(json!({
                "action": "account_history",
                "account": self.genesis_account,
                "count": "10"
            }))
            .await?;
        let grpc = self
            .account
            .account_history(AccountHistoryRequest {
                account: self.genesis_account.clone(),
                head: String::new(),
                count: 10,
                raw: false,
                reverse: false,
                account_filter: String::new(),
            })
            .await
            .map_err(display_status)?
            .into_inner();
        let json_entry = json
            .get("history")
            .and_then(Value::as_array)
            .and_then(|history| history.first())
            .ok_or_else(|| format!("JSON-RPC history entry missing: {json}"))?;
        let grpc_entry = grpc
            .entries
            .first()
            .ok_or_else(|| "gRPC history entry missing".to_string())?;

        compare("hash", &grpc_entry.hash, json_string(json_entry, "hash")?)?;
        compare("type", &grpc_entry.r#type, json_string(json_entry, "type")?)?;
        compare(
            "height",
            &grpc_entry.height,
            json_string(json_entry, "height")?,
        )
    }

    async fn account_history_invalid(&mut self) -> Result<(), String> {
        expect_code(
            self.account
                .account_history(AccountHistoryRequest {
                    account: INVALID_ACCOUNT.to_string(),
                    head: String::new(),
                    count: 10,
                    raw: false,
                    reverse: false,
                    account_filter: String::new(),
                })
                .await,
            Code::InvalidArgument,
        )
    }

    async fn account_representative_genesis(&mut self) -> Result<(), String> {
        let json = self
            .rpc
            .call(json!({
                "action": "account_representative",
                "account": self.genesis_account
            }))
            .await?;
        let grpc = self
            .account
            .account_representative(AccountRepresentativeRequest {
                account: self.genesis_account.clone(),
            })
            .await
            .map_err(display_status)?
            .into_inner();

        compare(
            "representative",
            &grpc.representative,
            json_string(&json, "representative")?,
        )
    }

    async fn account_representative_missing(&mut self) -> Result<(), String> {
        expect_code(
            self.account
                .account_representative(AccountRepresentativeRequest {
                    account: MISSING_ACCOUNT.to_string(),
                })
                .await,
            Code::NotFound,
        )
    }

    async fn process_invalid_block(&mut self) -> Result<(), String> {
        expect_code(
            self.block
                .process(ProcessRequest {
                    block: "{}".to_string(),
                    force: false,
                    watch_work: false,
                    subtype: String::new(),
                })
                .await,
            Code::InvalidArgument,
        )
    }

    async fn block_info_genesis(&mut self) -> Result<(), String> {
        let json = self
            .rpc
            .call(json!({
                "action": "block_info",
                "hash": self.genesis_hash,
                "json_block": "true"
            }))
            .await?;
        let grpc = self
            .block
            .block_info(BlockInfoRequest {
                hash: self.genesis_hash.clone(),
            })
            .await
            .map_err(display_status)?
            .into_inner();

        compare(
            "block_account",
            &grpc.block_account,
            json_string(&json, "block_account")?,
        )?;
        compare("amount", &grpc.amount, json_string(&json, "amount")?)?;
        compare("balance", &grpc.balance, json_string(&json, "balance")?)?;
        compare("height", &grpc.height, json_string(&json, "height")?)?;
        compare(
            "confirmed",
            &grpc.confirmed,
            json_string(&json, "confirmed")?,
        )
    }

    async fn block_info_missing(&mut self) -> Result<(), String> {
        expect_code(
            self.block
                .block_info(BlockInfoRequest {
                    hash: MISSING_HASH.to_string(),
                })
                .await,
            Code::NotFound,
        )
    }

    async fn blocks_genesis(&mut self) -> Result<(), String> {
        let json = self
            .rpc
            .call(json!({
                "action": "blocks",
                "hashes": [self.genesis_hash],
                "json_block": "true"
            }))
            .await?;
        let grpc = self
            .block
            .blocks(BlocksRequest {
                hashes: vec![self.genesis_hash.clone()],
                json_not_found: false,
            })
            .await
            .map_err(display_status)?
            .into_inner();
        let json_block = json
            .get("blocks")
            .and_then(Value::as_object)
            .and_then(|blocks| blocks.get(&self.genesis_hash))
            .ok_or_else(|| format!("JSON-RPC block missing: {json}"))?;
        let grpc_block = grpc
            .blocks
            .get(&self.genesis_hash)
            .ok_or_else(|| "gRPC block missing".to_string())?;
        let grpc_json: Value = serde_json::from_str(grpc_block)
            .map_err(|error| format!("gRPC block is not valid JSON: {error}"))?;

        if &grpc_json == json_block {
            Ok(())
        } else {
            Err(format!(
                "block contents differ: gRPC={grpc_json}, JSON-RPC={json_block}"
            ))
        }
    }

    async fn blocks_missing(&mut self) -> Result<(), String> {
        let json = self
            .rpc
            .call(json!({ "action": "blocks", "hashes": [MISSING_HASH] }))
            .await?;
        if json.get("error").is_none() {
            return Err(format!(
                "JSON-RPC unexpectedly returned missing block: {json}"
            ));
        }

        expect_code(
            self.block
                .blocks(BlocksRequest {
                    hashes: vec![MISSING_HASH.to_string()],
                    json_not_found: false,
                })
                .await,
            Code::NotFound,
        )
    }

    async fn frontier_count(&mut self) -> Result<(), String> {
        let json = self.rpc.call(json!({ "action": "frontier_count" })).await?;
        let grpc = self
            .ledger
            .frontier_count(FrontierCountRequest {})
            .await
            .map_err(display_status)?
            .into_inner();

        compare(
            "count",
            &grpc.count.to_string(),
            json_string(&json, "count")?,
        )
    }

    async fn receivable_empty(&mut self) -> Result<(), String> {
        let json = self
            .rpc
            .call(json!({
                "action": "receivable",
                "account": self.genesis_account,
                "count": "10",
                "source": "true"
            }))
            .await?;
        let grpc = self
            .ledger
            .receivable_blocks(ReceivableBlocksRequest {
                account: self.genesis_account.clone(),
                count: 10,
                threshold: "0".to_string(),
                source: true,
                include_active: false,
                sorting: false,
                include_only_confirmed: true,
            })
            .await
            .map_err(display_status)?
            .into_inner();
        let json_count = json
            .get("blocks")
            .and_then(Value::as_object)
            .map(|blocks| blocks.len())
            .ok_or_else(|| format!("JSON-RPC blocks map missing: {json}"))?;

        compare(
            "receivable block count",
            &grpc.blocks.len().to_string(),
            &json_count.to_string(),
        )
    }

    async fn receivable_invalid_account(&mut self) -> Result<(), String> {
        expect_code(
            self.ledger
                .receivable_blocks(ReceivableBlocksRequest {
                    account: INVALID_ACCOUNT.to_string(),
                    count: 10,
                    threshold: "0".to_string(),
                    source: true,
                    include_active: false,
                    sorting: false,
                    include_only_confirmed: true,
                })
                .await,
            Code::InvalidArgument,
        )
    }

    async fn peers(&mut self) -> Result<(), String> {
        let json = self.rpc.call(json!({ "action": "peers" })).await?;
        let grpc = self
            .network
            .peers(PeersRequest {})
            .await
            .map_err(display_status)?
            .into_inner();
        let json_count = json
            .get("peers")
            .and_then(Value::as_object)
            .map(|peers| peers.len())
            .ok_or_else(|| format!("JSON-RPC peers map missing: {json}"))?;

        compare(
            "peer count",
            &grpc.peers.len().to_string(),
            &json_count.to_string(),
        )
    }

    async fn telemetry(&mut self) -> Result<(), String> {
        let json = self.rpc.call(json!({ "action": "telemetry" })).await?;
        let grpc = self
            .network
            .telemetry(TelemetryRequest {
                address: String::new(),
                port: 0,
            })
            .await
            .map_err(display_status)?
            .into_inner();

        compare(
            "block_count",
            &grpc.block_count,
            json_string(&json, "block_count")?,
        )?;
        compare(
            "cemented_count",
            &grpc.cemented_count,
            json_string(&json, "cemented_count")?,
        )?;
        compare(
            "account_count",
            &grpc.account_count,
            json_string(&json, "account_count")?,
        )?;
        compare(
            "genesis_block",
            &grpc.genesis_block,
            json_string(&json, "genesis_block")?,
        )
    }

    async fn status(&mut self) -> Result<(), String> {
        let json = self.rpc.call(json!({ "action": "block_count" })).await?;
        let grpc = self
            .node
            .status(StatusRequest {})
            .await
            .map_err(display_status)?
            .into_inner();

        compare(
            "block_count",
            &grpc.block_count.to_string(),
            json_string(&json, "count")?,
        )?;
        compare(
            "cemented_count",
            &grpc.cemented_count,
            json_string(&json, "cemented")?,
        )?;
        compare("genesis_block", &grpc.genesis_block, &self.genesis_hash)
    }

    async fn version(&mut self) -> Result<(), String> {
        let json = self.rpc.call(json!({ "action": "version" })).await?;
        let grpc = self
            .node
            .version(VersionRequest {})
            .await
            .map_err(display_status)?
            .into_inner();

        compare(
            "rpc_version",
            &grpc.rpc_version,
            json_string(&json, "rpc_version")?,
        )?;
        compare(
            "store_version",
            &grpc.store_version,
            json_string(&json, "store_version")?,
        )?;
        compare(
            "protocol_version",
            &grpc.protocol_version,
            json_string(&json, "protocol_version")?,
        )?;
        compare(
            "network_identifier",
            &grpc.network_identifier,
            json_string(&json, "network_identifier")?,
        )
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let config = match Config::from_args() {
        Ok(config) => config,
        Err(error) if error.is_empty() => return ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };

    let mut report = Report::new(&config.rpc_url, &config.grpc_url);
    match Harness::connect(&config).await {
        Ok(mut harness) => run_scenarios(&mut harness, &mut report).await,
        Err(error) => report.set_setup_error(error),
    }

    let markdown = report.render_markdown();
    if let Err(error) = write_report(&config.report_path, &markdown) {
        eprintln!("{error}");
        return ExitCode::from(2);
    }

    println!("wrote {}", config.report_path.display());
    if report.has_failures() {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

async fn run_scenarios(harness: &mut Harness, report: &mut Report) {
    report.record(ScenarioResult::new(
        ACCOUNT_INFO,
        "genesis account response",
        harness.account_info_genesis().await,
    ));
    report.record(ScenarioResult::new(
        ACCOUNT_INFO,
        "invalid account status",
        harness.account_info_invalid().await,
    ));
    report.record(ScenarioResult::new(
        ACCOUNT_INFO,
        "unopened account status",
        harness.account_info_missing().await,
    ));
    report.record(ScenarioResult::new(
        ACCOUNT_BALANCE,
        "genesis account balances",
        harness.account_balance_genesis().await,
    ));
    report.record(ScenarioResult::new(
        ACCOUNT_BALANCE,
        "unopened account zero balances",
        harness.account_balance_unopened().await,
    ));
    report.record(ScenarioResult::new(
        ACCOUNT_HISTORY,
        "genesis account history shape",
        harness.account_history_genesis().await,
    ));
    report.record(ScenarioResult::new(
        ACCOUNT_HISTORY,
        "invalid account status",
        harness.account_history_invalid().await,
    ));
    report.record(ScenarioResult::new(
        ACCOUNT_REPRESENTATIVE,
        "genesis representative",
        harness.account_representative_genesis().await,
    ));
    report.record(ScenarioResult::new(
        ACCOUNT_REPRESENTATIVE,
        "unopened account status",
        harness.account_representative_missing().await,
    ));
    report.record(ScenarioResult::new(
        PROCESS,
        "invalid block status",
        harness.process_invalid_block().await,
    ));
    report.record(ScenarioResult::new(
        BLOCK_INFO,
        "genesis block response",
        harness.block_info_genesis().await,
    ));
    report.record(ScenarioResult::new(
        BLOCK_INFO,
        "missing block status",
        harness.block_info_missing().await,
    ));
    report.record(ScenarioResult::new(
        BLOCKS,
        "genesis block response",
        harness.blocks_genesis().await,
    ));
    report.record(ScenarioResult::new(
        BLOCKS,
        "missing block status",
        harness.blocks_missing().await,
    ));
    report.record(ScenarioResult::new(
        FRONTIER_COUNT,
        "development ledger count",
        harness.frontier_count().await,
    ));
    report.record(ScenarioResult::new(
        RECEIVABLE_BLOCKS,
        "empty receivable response",
        harness.receivable_empty().await,
    ));
    report.record(ScenarioResult::new(
        RECEIVABLE_BLOCKS,
        "invalid account status",
        harness.receivable_invalid_account().await,
    ));
    report.record(ScenarioResult::new(
        PEERS,
        "empty development peer set",
        harness.peers().await,
    ));
    report.record(ScenarioResult::new(
        TELEMETRY,
        "local telemetry response",
        harness.telemetry().await,
    ));
    report.record(ScenarioResult::new(
        STATUS,
        "development node status",
        harness.status().await,
    ));
    report.record(ScenarioResult::new(
        VERSION,
        "node version response",
        harness.version().await,
    ));
}

fn write_report(path: &Path, markdown: &str) -> Result<(), String> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("create report directory: {error}"))?;
    }
    std::fs::write(path, markdown).map_err(|error| format!("write report: {error}"))
}

fn json_string<'a>(value: &'a Value, field: &str) -> Result<&'a str, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("JSON-RPC field `{field}` missing or not a string: {value}"))
}

fn compare(label: &str, grpc: &str, json_rpc: &str) -> Result<(), String> {
    if grpc == json_rpc {
        Ok(())
    } else {
        Err(format!(
            "{label} differs: gRPC=`{grpc}`, JSON-RPC=`{json_rpc}`"
        ))
    }
}

fn expect_code<T>(
    result: Result<tonic::Response<T>, Status>,
    expected: Code,
) -> Result<(), String> {
    match result {
        Err(status) if status.code() == expected => Ok(()),
        Err(status) => Err(format!(
            "expected gRPC status {expected:?}, got {:?}: {}",
            status.code(),
            status.message()
        )),
        Ok(_) => Err(format!(
            "expected gRPC status {expected:?}, request succeeded"
        )),
    }
}

fn display_status(status: Status) -> String {
    format!("gRPC {:?}: {}", status.code(), status.message())
}

fn display_result<T, E: std::fmt::Display>(result: Result<T, E>) -> String {
    match result {
        Ok(_) => "ready".to_string(),
        Err(error) => error.to_string(),
    }
}

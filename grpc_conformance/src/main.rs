use std::env;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use reqwest::Client;
use rsnano_grpc_conformance::{Report, ScenarioResult};
use rsnano_grpc_proto::nano::v1::{
    FrontierCountRequest, GetAccountStateRequest, GetBlockRequest, GetBlockStatusesRequest,
    ListAccountHistoryRequest, ListReceivablesRequest, ReceivableMode,
    RequestBlockConfirmationRequest, account_service_client::AccountServiceClient,
    block_service_client::BlockServiceClient, ledger_service_client::LedgerServiceClient,
};
use rsnano_ledger::{DEV_GENESIS_ACCOUNT, DEV_GENESIS_HASH};
use serde_json::{Value, json};
use tonic::transport::{Channel, Endpoint};
use tonic::{Code, Status};

const ACCOUNT_STATE: &str = "AccountService.GetAccountState";
const ACCOUNT_HISTORY: &str = "AccountService.ListAccountHistory";
const GET_BLOCK: &str = "BlockService.GetBlock";
const BLOCK_STATUSES: &str = "BlockService.GetBlockStatuses";
const REQUEST_CONFIRMATION: &str = "BlockService.RequestBlockConfirmation";
const FRONTIER_COUNT: &str = "LedgerService.FrontierCount";
const RECEIVABLES: &str = "LedgerService.ListReceivables";
const INVALID_ACCOUNT: &str = "not-a-nano-account";
const MISSING_HASH: &str = "1111111111111111111111111111111111111111111111111111111111111111";

struct Config {
    rpc_url: String,
    grpc_url: String,
    report_path: PathBuf,
}

impl Config {
    fn from_args() -> Result<Self, String> {
        let mut config = Self {
            rpc_url: "http://[::1]:45000".to_string(),
            grpc_url: "http://[::1]:47078".to_string(),
            report_path: "grpc_conformance/reports/current.md".into(),
        };
        let mut args = env::args().skip(1);
        while let Some(arg) = args.next() {
            let value = || format!("{arg} requires a value");
            match arg.as_str() {
                "--rpc-url" => config.rpc_url = args.next().ok_or_else(value)?,
                "--grpc-url" => config.grpc_url = args.next().ok_or_else(value)?,
                "--report" => config.report_path = args.next().ok_or_else(value)?.into(),
                "--help" | "-h" => return Err(String::new()),
                _ => return Err(format!("unknown argument: {arg}")),
            }
        }
        Ok(config)
    }
}

struct Harness {
    rpc_url: String,
    http: Client,
    account: AccountServiceClient<Channel>,
    block: BlockServiceClient<Channel>,
    ledger: LedgerServiceClient<Channel>,
    account_address: String,
    genesis_hash: String,
}

impl Harness {
    async fn connect(config: &Config) -> Result<Self, String> {
        let http = Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .map_err(|e| e.to_string())?;
        let endpoint = Endpoint::from_shared(config.grpc_url.clone())
            .map_err(|e| e.to_string())?
            .connect_timeout(Duration::from_secs(2));
        for _ in 0..60 {
            if let Ok(channel) = endpoint.connect().await {
                return Ok(Self {
                    rpc_url: config.rpc_url.clone(),
                    http,
                    account: AccountServiceClient::new(channel.clone()),
                    block: BlockServiceClient::new(channel.clone()),
                    ledger: LedgerServiceClient::new(channel),
                    account_address: DEV_GENESIS_ACCOUNT.encode_account(),
                    genesis_hash: DEV_GENESIS_HASH.to_string(),
                });
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        Err("gRPC endpoint was not ready after 30 seconds".to_string())
    }

    async fn rpc(&self, value: Value) -> Result<Value, String> {
        self.http
            .post(&self.rpc_url)
            .json(&value)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())
    }

    async fn account_state(&mut self) -> Result<(), String> {
        let rpc = self.rpc(json!({"action":"account_info","account":self.account_address,"representative":"true"})).await?;
        let grpc = self
            .account
            .get_account_state(GetAccountStateRequest {
                account: self.account_address.clone(),
            })
            .await
            .map_err(status)?
            .into_inner();
        let current = grpc.current.ok_or("missing current account state")?;
        compare(&current.frontier, field(&rpc, "frontier")?)?;
        compare(&current.balance_raw, field(&rpc, "balance")?)?;
        compare(&current.representative, field(&rpc, "representative")?)?;
        compare(
            &current.block_count.to_string(),
            field(&rpc, "block_count")?,
        )
    }

    async fn invalid_account(&mut self) -> Result<(), String> {
        expect_code(
            self.account
                .get_account_state(GetAccountStateRequest {
                    account: INVALID_ACCOUNT.to_string(),
                })
                .await,
            Code::InvalidArgument,
        )
    }

    async fn account_history(&mut self) -> Result<(), String> {
        let response = self
            .account
            .list_account_history(ListAccountHistoryRequest {
                account: self.account_address.clone(),
                head: None,
                limit: 1,
                ascending: false,
            })
            .await
            .map_err(status)?
            .into_inner();
        compare(
            &response.blocks.first().ok_or("empty history")?.block_hash,
            &self.genesis_hash,
        )
    }

    async fn get_block(&mut self) -> Result<(), String> {
        let rpc = self
            .rpc(json!({"action":"block_info","hash":self.genesis_hash,"json_block":"true"}))
            .await?;
        let record = self
            .block
            .get_block(GetBlockRequest {
                block_hash: self.genesis_hash.clone(),
            })
            .await
            .map_err(status)?
            .into_inner()
            .block
            .ok_or("missing block")?;
        compare(&record.block_hash, &self.genesis_hash)?;
        compare(
            &record
                .sideband
                .ok_or("missing sideband")?
                .resulting_balance_raw,
            field(&rpc, "balance")?,
        )
    }

    async fn statuses(&mut self) -> Result<(), String> {
        let response = self
            .block
            .get_block_statuses(GetBlockStatusesRequest {
                block_hashes: vec![self.genesis_hash.clone(), MISSING_HASH.to_string()],
            })
            .await
            .map_err(status)?
            .into_inner();
        if response.statuses.len() != 2
            || response.statuses[0].local_state != 3
            || response.statuses[1].local_state != 1
        {
            return Err(format!(
                "unexpected ordered statuses: {:?}",
                response.statuses
            ));
        }
        Ok(())
    }

    async fn missing_confirmation(&mut self) -> Result<(), String> {
        expect_code(
            self.block
                .request_block_confirmation(RequestBlockConfirmationRequest {
                    block_hash: MISSING_HASH.to_string(),
                })
                .await,
            Code::FailedPrecondition,
        )
    }

    async fn frontier_count(&mut self) -> Result<(), String> {
        let rpc = self.rpc(json!({"action":"frontier_count"})).await?;
        let grpc = self
            .ledger
            .frontier_count(FrontierCountRequest {})
            .await
            .map_err(status)?
            .into_inner();
        compare(&grpc.count.to_string(), field(&rpc, "count")?)
    }

    async fn receivables(&mut self) -> Result<(), String> {
        self.ledger
            .list_receivables(ListReceivablesRequest {
                account: self.account_address.clone(),
                limit: 10,
                minimum_amount_raw: "0".to_string(),
                mode: ReceivableMode::ConfirmedOnly as i32,
            })
            .await
            .map_err(status)?;
        Ok(())
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let config = match Config::from_args() {
        Ok(config) => config,
        Err(error) => {
            if !error.is_empty() {
                eprintln!("{error}");
            }
            return ExitCode::from(2);
        }
    };
    let mut report = Report::new(&config.rpc_url, &config.grpc_url);
    match Harness::connect(&config).await {
        Ok(mut harness) => {
            macro_rules! scenario {
                ($method:expr, $name:expr, $call:expr) => {
                    report.record(ScenarioResult::new($method, $name, $call.await));
                };
            }
            scenario!(
                ACCOUNT_STATE,
                "genesis current and confirmed snapshot",
                harness.account_state()
            );
            scenario!(
                ACCOUNT_STATE,
                "invalid account status",
                harness.invalid_account()
            );
            scenario!(
                ACCOUNT_HISTORY,
                "account-chain-local genesis history",
                harness.account_history()
            );
            scenario!(GET_BLOCK, "typed genesis block", harness.get_block());
            scenario!(
                BLOCK_STATUSES,
                "ordered cemented and missing statuses",
                harness.statuses()
            );
            scenario!(
                REQUEST_CONFIRMATION,
                "missing local prerequisite",
                harness.missing_confirmation()
            );
            scenario!(
                FRONTIER_COUNT,
                "deterministic ledger count",
                harness.frontier_count()
            );
            scenario!(
                RECEIVABLES,
                "confirmed-only default-compatible query",
                harness.receivables()
            );
        }
        Err(error) => report.set_setup_error(error),
    }
    let markdown = report.render_markdown();
    if let Some(parent) = config.report_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(error) = std::fs::write(&config.report_path, markdown) {
        eprintln!("write report: {error}");
        return ExitCode::FAILURE;
    }
    if report.has_failures() {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn field<'a>(value: &'a Value, name: &str) -> Result<&'a str, String> {
    value
        .get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing JSON-RPC field {name}: {value}"))
}
fn compare(actual: &str, expected: &str) -> Result<(), String> {
    if actual == expected {
        Ok(())
    } else {
        Err(format!("expected {expected}, got {actual}"))
    }
}
fn status(value: Status) -> String {
    format!("{}: {}", value.code(), value.message())
}
fn expect_code<T>(result: Result<T, Status>, expected: Code) -> Result<(), String> {
    match result {
        Err(error) if error.code() == expected => Ok(()),
        Err(error) => Err(format!("expected {expected}, got {}", error.code())),
        Ok(_) => Err(format!("expected {expected}, call succeeded")),
    }
}

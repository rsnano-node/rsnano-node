use anyhow::Result;
use rand::Rng;
use rsnano_core::{Account, Amount, WalletId};
use rsnano_ledger::DEV_GENESIS_ACCOUNT;
use rsnano_rpc_client::NanoRpcClient;
use rsnano_rpc_messages::{AccountInfoDto, KeyPairDto};
use std::{
    collections::HashMap,
    io::Write,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{spawn, sync::Semaphore, time::sleep};

pub async fn create_send_and_receive_blocks(
    send_count: usize,
    simultaneous_process_calls: usize,
    destination_accounts: Vec<KeyPairDto>,
    wallet: WalletId,
    node_client: Arc<NanoRpcClient>,
) -> Result<HashMap<Account, AccountInfoDto>> {
    let factory = Arc::new(BlockFactory {
        send_count,
        simultaneous_calls_semaphore: Arc::new(Semaphore::new(simultaneous_process_calls)),
        send_calls_remaining: AtomicUsize::new(send_count),
        destination_accounts,
        wallet,
        node_client,
    });

    let f1 = factory.clone();
    let f2 = factory.clone();

    let (send_result, wait_result) = tokio::join!(spawn(send_loop(f1)), spawn(show_progress(f2)));
    send_result??;
    wait_result??;

    get_account_info(&factory.node_client, &factory.destination_accounts).await
}

// Send from genesis account to different accounts and receive the funds
async fn send_loop(block_factory: Arc<BlockFactory>) -> Result<()> {
    let mut join_handles = Vec::with_capacity(block_factory.send_count);
    for i in 0..block_factory.send_count {
        let block_factory = block_factory.clone();

        let handle = spawn(async move { block_factory.send_and_receive(i).await });
        join_handles.push(handle);
    }

    for h in join_handles {
        h.await??;
    }
    Ok(())
}

struct BlockFactory {
    send_count: usize,
    simultaneous_calls_semaphore: Arc<Semaphore>,
    send_calls_remaining: AtomicUsize,
    destination_accounts: Vec<KeyPairDto>,
    wallet: WalletId,
    node_client: Arc<NanoRpcClient>,
}

impl BlockFactory {
    fn is_done(&self) -> bool {
        self.send_calls_remaining.load(Ordering::SeqCst) == 0
    }

    fn percent_done(&self) -> i32 {
        (self.completed_sends() as f64 / self.send_count as f64 * 100_f64) as i32
    }

    fn completed_sends(&self) -> usize {
        self.send_count - self.send_calls_remaining.load(Ordering::SeqCst)
    }

    fn get_destination_account(&self, send_no: usize) -> &KeyPairDto {
        if send_no < self.destination_accounts.len() {
            &self.destination_accounts[send_no]
        } else {
            let random_account_index =
                rand::thread_rng().gen_range(0..self.destination_accounts.len());
            &self.destination_accounts[random_account_index]
        }
    }

    async fn send_and_receive(&self, send_no: usize) -> Result<()> {
        let _permit = Arc::clone(&self.simultaneous_calls_semaphore)
            .acquire_owned()
            .await?;
        let destination_account = self.get_destination_account(send_no);

        let res = self
            .node_client
            .send_receive(
                self.wallet,
                *DEV_GENESIS_ACCOUNT,
                destination_account.account,
                Amount::raw(1),
            )
            .await;
        self.send_calls_remaining.fetch_sub(1, Ordering::SeqCst);
        res
    }
}

async fn get_account_info(
    node_client: &NanoRpcClient,
    accounts: &[KeyPairDto],
) -> Result<HashMap<Account, AccountInfoDto>> {
    let mut account_info = HashMap::new();
    for account in accounts {
        let account = account.account;
        account_info.insert(account, node_client.account_info(account).await?);
    }
    Ok(account_info)
}

async fn show_progress(factory: Arc<BlockFactory>) -> Result<()> {
    print!("\rPrimary node processing transactions: 00%");
    std::io::stdout().flush()?;

    let mut last_percent = 0;
    while !factory.is_done() {
        let percent = factory.percent_done();
        if last_percent != percent {
            print!("\rPrimary node processing transactions: {:02}%", percent,);
            std::io::stdout().flush()?;
            last_percent = percent;
        }
        sleep(Duration::from_millis(100)).await;
    }
    println!("\rPrimary node processed transactions                ");
    Ok(())
}

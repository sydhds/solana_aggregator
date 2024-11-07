use std::collections::HashSet;
use anyhow::anyhow;
use flume::{Receiver, Sender};
use futures_util::StreamExt;
use schnellru::{ByLength, LruMap};
use solana_client::nonblocking::pubsub_client::PubsubClient;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::account::Account;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::{AbortHandle, JoinSet};
use tracing::{
    // debug,
    info,
    warn
};

use sol_db::{actions_account, models, DbPool};

/// Solana account collector
pub struct AccountCollector {
    pub ws_client: Arc<PubsubClient>,
    pub rpc_client: Arc<RpcClient>,
    pub db_pool: DbPool,
    // cfg
    pub max_account_to_monitor: u32,
    pub get_account_delay: Duration,
    pub unsubscribe_delay: Duration,
    pub account_writer_count: u32,
}

pub enum AccountChange {
    New(String, u64, String),
    Update(String, u64, String),
}

impl AccountCollector {
    /// Main account collector worker
    pub async fn run(
        &self,
        rx_accounts: Receiver<Vec<String>>,
    ) -> anyhow::Result<()> {
        let mut accounts_map: LruMap<String, AbortHandle, _> =
            LruMap::new(ByLength::new(self.max_account_to_monitor));
        let (tx_to_monitor, rx_to_monitor) = flume::unbounded();
        let mut account_monitor_tasks = JoinSet::new();
        let mut account_writer_tasks = JoinSet::new();
        let (tx_account_update, rx_account_update) = flume::unbounded();
        let (tx_account_db, rx_account_db) = flume::unbounded();

        // Spawn account db writer workers
        for _i in 0..self.account_writer_count {
            let _ = account_writer_tasks.spawn(AccountCollector::account_writer_worker(
                rx_account_db.clone(),
                self.db_pool.clone(),
            ));
        }

        loop {
            tokio::select! {
                accounts_ = rx_accounts.recv_async() => {
                    // Received account strings
                    if accounts_.is_err() || rx_accounts.is_disconnected() {
                        warn!("accounts_ is err or disco...");
                        break;
                        // continue;
                    }
                    let accounts = accounts_.unwrap(); // safe to unwrap

                    let unknown_accounts: HashSet<String> = accounts
                        .into_iter()
                        .filter(|account| {
                            accounts_map.get(account).is_none()
                        })
                        .collect();

                    // Spawn task to get account info + write DB
                    for unknown_account in unknown_accounts {
                        tokio::task::spawn(
                            AccountCollector::new_account_worker(
                                self.rpc_client.clone(),
                                unknown_account,
                                tx_to_monitor.clone(),
                                tx_account_db.clone(),
                                self.get_account_delay
                            )
                        );
                    }
                },
                to_monitor_ = rx_to_monitor.recv_async() => {
                    if to_monitor_.is_err() || rx_to_monitor.is_disconnected() {
                        warn!("to_monitor_ is err or disco...");
                        break;
                    }
                    let (account, account_pubkey, solana_account_) = to_monitor_.unwrap();

                    if let Some(_solana_account) = solana_account_ {
                        let ws_client = self.ws_client.clone();
                        let abort_handle = account_monitor_tasks.spawn(
                            AccountCollector::account_monitor_worker(ws_client,
                                account.clone(),
                                account_pubkey,
                                tx_account_update.clone(),
                                tx_account_db.clone(),
                                self.unsubscribe_delay
                            )
                        );

                        // Note: safe to do 'as u32' here as accounts_map limit is self.max_account_to_monitor
                        if accounts_map.len() as u32 >= self.max_account_to_monitor {
                            if let Some((account, abort_handle)) = accounts_map.pop_oldest() {
                                info!("Stop monitoring for account: {}", account);
                                abort_handle.abort();
                            }
                        }
                        accounts_map.insert(account, abort_handle);
                    }
                }
                account_update_ = rx_account_update.recv_async() => {
                    if account_update_.is_err() || rx_account_update.is_disconnected() {
                        warn!("account_update_ is err or disco...");
                        break;
                    }
                    let account_update = account_update_.unwrap();
                    // Account has been updated so promote it in LRUMap
                    accounts_map.get(&account_update);
                }
            }
        }

        info!("Exiting account collector...");
        // Cleanup a bit
        drop(tx_to_monitor);
        drop(tx_account_update);
        drop(tx_account_db);
        account_monitor_tasks.abort_all();
        account_writer_tasks.abort_all();
        Ok(())
    }

    /// Worker to fetch account info
    async fn new_account_worker(
        rpc_client: Arc<RpcClient>,
        account: String,
        tx: Sender<(String, Pubkey, Option<Account>)>,
        tx_account_db: Sender<AccountChange>,
        get_account_delay: Duration,
    ) -> anyhow::Result<()> {
        let account_pubkey_ = Pubkey::from_str(account.as_str());

        if let Ok(account_pubkey) = account_pubkey_ {
            let solana_account_ = tokio::time::timeout(
                get_account_delay,
                rpc_client.get_account(&account_pubkey)
            ).await;
            
            println!("solana account: {:?}", solana_account_);

            if let Ok(Ok(solana_account)) = solana_account_ {
                // info!("Get solana account: {:?}", solana_account);
                let account_id = account.clone();

                // Do not use tokio::join! here as we want to first write to db
                // then start a monitoring worker
                let _ = tx_account_db
                    .send_async(AccountChange::New(
                        account_id,
                        solana_account.lamports,
                        solana_account.owner.to_string().clone(),
                    ))
                    .await;
                // info!("Now send to monitor...");
                let _ = tx
                    .send_async((account, account_pubkey, Some(solana_account)))
                    .await;
            } else {
                // info!("Only send to monitor to None...");
                let _ = tx.send_async((account, account_pubkey, None)).await;
                return Err(anyhow!("Failed or timeout rpc get_account"))
            }
        } else {
            return Err(anyhow!("Invalid account pubkey"))
        }

        Ok(())
    }

    /// Monitor account
    async fn account_monitor_worker(
        ws_client: Arc<PubsubClient>,
        account: String,
        account_pubkey: Pubkey,
        tx_account_update: Sender<String>,
        tx_account_db: Sender<AccountChange>,
        unsubscribe_delay: Duration
    ) {
        if let Ok((mut acc_sub, acc_unsubscriber)) =
            ws_client.account_subscribe(&account_pubkey, None).await
        {
            while let Some(response) = acc_sub.next().await {
                // debug!("response: {:?}", response);
                let ui_account = response.value;

                let _ = tokio::join!(
                    tx_account_update.send_async(account.clone()), // Notify main loop for account up
                    tx_account_db.send_async(AccountChange::Update(
                        account.clone(),
                        ui_account.lamports,
                        ui_account.owner.to_string().clone()
                    ))  // Write update to db
                );
            }

            info!("Exiting account monitor worker...");
            let _ = tokio::time::timeout(
                unsubscribe_delay,
                acc_unsubscriber()
            ).await;
        }
    }

    /// Write account change to db
    async fn account_writer_worker(rx: Receiver<AccountChange>, db_pool: DbPool) {
        loop {
            let account_update_ = rx.recv_async().await;
            if account_update_.is_err() || rx.is_disconnected() {
                warn!("account_update_ is err or is_disco...");
                break;
            }

            let account_update = account_update_.unwrap();
            let db_pool = db_pool.clone();

            match account_update {
                AccountChange::New(acc_id, lamports, owner) => {
                    let account = models::Account {
                        id: acc_id,
                        lamports: lamports.to_string(),
                        owner,
                    };
                    let db_insert = tokio::task::spawn_blocking(move || {
                        let mut conn = db_pool.get().map_err(|e| anyhow!(e))?;
                        actions_account::insert_new_account(&mut conn, account)
                    })
                    .await;

                    info!("New account (insert): {:?}", db_insert);
                }
                AccountChange::Update(acc_id, lamports, owner) => {
                    let account = models::Account {
                        id: acc_id,
                        lamports: lamports.to_string(),
                        owner,
                    };

                    let db_update = tokio::task::spawn_blocking(move || {
                        let mut conn = db_pool.get().map_err(|e| anyhow!(e))?;
                        actions_account::update_account(&mut conn, account)
                    })
                    .await;

                    info!("Account updated: {:?}", db_update);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_account_monitor_worker() {

        // Create an `RpcClient` that always fails
        let url = "fails".to_string();
        let client = Arc::new(RpcClient::new_mock(url));
        let unsubscribe_delay = Duration::from_secs(1);
        let account = "Cwg1f6m4m3DGwMEbmsbAfDtUToUf5jRdKrJSGD7GfZCB".to_string();
        let account_pubkey = Pubkey::from_str(account.as_str()).unwrap();

        let (tx_account_update, rx_account_update) = flume::unbounded();
        let (tx_account_db, rx_account_db) = flume::unbounded();
        
        let worker = AccountCollector::new_account_worker(
            client,
            account,
            tx_account_update.clone(),
            tx_account_db.clone(),
            unsubscribe_delay
        );

        let res = worker.await;
        // println!("res: {:?}", res);
        assert!(res.is_err());
    }

}
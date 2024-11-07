mod utils;

use anyhow::anyhow;
use flume::{Receiver, Sender};
use futures_util::future::BoxFuture;
use futures_util::StreamExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinSet;

use solana_client::nonblocking::pubsub_client::PubsubClient;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_client::rpc_config::{
    RpcSignatureSubscribeConfig, RpcTransactionLogsConfig, RpcTransactionLogsFilter,
};
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::signature::Signature;
use solana_transaction_status::{
    EncodedConfirmedTransactionWithStatusMeta, EncodedTransaction, UiMessage, UiTransactionEncoding,
};
use tracing::{
    // debug,
    error,
    info,
    warn
};

use crate::utils::signature_decode;
use sol_db::models::Transaction;
use sol_db::{actions, DbPool};

/// Solana tx collector
pub struct TxCollector {
    pub ws_client: Arc<PubsubClient>,
    pub rpc_client: Arc<RpcClient>,
    pub db_pool: DbPool,
    pub shutdown_marker: Arc<AtomicBool>,
    // cfg
    pub writer_worker_count: u32,
    pub process_worker_count: u32,
}

impl TxCollector {
    /// Main Tx collector worker
    pub async fn run(&self, tx_accounts: Sender<Vec<String>>) -> anyhow::Result<()> {
        // Setup
        let (tx_logs, rx_logs) = flume::unbounded();
        let (tx_db, rx_db) = flume::unbounded();

        // Spawn other tasks

        let mut process_incoming_tx_set = JoinSet::new();
        // Spawn N tasks to process incoming transactions
        for _i in 0..self.process_worker_count {
            process_incoming_tx_set.spawn(TxCollector::process_tx(
                self.ws_client.clone(),
                self.rpc_client.clone(),
                rx_logs.clone(),
                tx_db.clone(),
                tx_accounts.clone(),
            ));
        }
        // Spawn N tasks to write tx to db
        let mut writer_tx_set = JoinSet::new();
        for _i in 0..self.writer_worker_count {
            writer_tx_set.spawn(TxCollector::write_tx(rx_db.clone(), self.db_pool.clone()));
        }

        let filter = RpcTransactionLogsFilter::All;
        let config = RpcTransactionLogsConfig { commitment: None };

        let (mut logs_sub, unsubscriber) = self
            .ws_client
            .logs_subscribe(filter, config).await?;

        while let Some(response) = logs_sub.next().await {

            // debug!("{:?}", response);

            if let Err(e) = tx_logs
                .send_async((response.value.signature, response.value.err.is_some()))
                .await
            {
                error!("Cannot send to queue, error={}", e.to_string());
                break;
            }

            // Detect Ctrl-C and exit
            if self.shutdown_marker.load(Ordering::SeqCst) {
                break;
            }
        }

        info!("Exiting tx collector...");
        // Drop queue early to let other task(s) to shut down
        drop(tx_logs);
        unsubscriber().await;
        Ok(())
    }

    /// Process tx worker
    async fn process_tx(
        ws_client: Arc<PubsubClient>,
        rpc_client: Arc<RpcClient>,
        rx: Receiver<(String, bool)>,
        tx_db: Sender<Transaction>,
        tx_accounts: Sender<Vec<String>>,
    ) -> anyhow::Result<()> {
        let commitment = CommitmentConfig::confirmed();
        let wait_for_tx_until_delay = Duration::from_secs(3); // TODO config
        let get_transaction_delay = Duration::from_secs(3); // TODO config

        loop {
            let res_ = rx.recv_async().await;
            if res_.is_err() || rx.is_disconnected() {
                // Note: sometimes rx is disconnected but does not return an error
                // All senders have been dropped - regular exit
                warn!("res_ is err or disco...");
                break;
            }

            let (tx_sig_str, tx_is_err) = res_.unwrap(); // safe to unwrap - already checked
            let tx_sig_ = signature_decode(tx_sig_str);

            if let Err(e) = tx_sig_.as_ref() {
                warn!("Unable to decode signature, error={}", e.to_string());
            }
            let tx_sig = tx_sig_.unwrap(); // safe to unwrap - already checked

            if !tx_is_err {
                // info!("Waiting for tx to be confirmed, tx_sig: {}", tx_sig);

                // wait for tx status = confirmed
                match tokio::time::timeout(
                    wait_for_tx_until_delay,
                    TxCollector::wait_for_tx_until(ws_client.clone(), tx_sig, commitment),
                )
                .await
                {
                    Err(_e) => {
                        // warn!("wait_for_tx_until timeout after {} secs", _e);
                    }
                    Ok(Ok(unsub_fn)) => {
                        // Spawn a task to call unsubscribe
                        tokio::task::spawn(async move {
                            tokio::time::timeout(Duration::from_secs(1), unsub_fn()).await
                        });
                    }
                    _ => {}
                }
            }

            // rpc get_transaction
            match tokio::time::timeout(
                get_transaction_delay,
                rpc_client.get_transaction(&tx_sig, UiTransactionEncoding::JsonParsed),
            )
            .await
            {
                Ok(Ok(tx_data)) => {
                    let (tx, accounts) = process_transaction(&tx_sig, tx_data);

                    let _ = tx_db.send_async(tx).await; // TODO: log error?
                                                        // Note: tokio wait for both if bounded
                    let _ = tx_accounts.send(accounts);
                }
                Ok(Err(_e)) => {
                    error!("get_transaction failed, sig={}, error={}", tx_sig, _e);
                    // TODO: create tx entry in db
                }
                Err(_e) => {
                    // warn!("get_transaction timeout after {} secs", get_transaction_delay.as_secs());
                }
            }
        }

        drop(tx_db);
        Ok(())
    }

    /// Wait for tx until it reaches commitment worker
    async fn wait_for_tx_until(
        client: Arc<PubsubClient>,
        tx_sig: Signature,
        commitment: CommitmentConfig,
    ) -> anyhow::Result<Box<dyn FnOnce() -> BoxFuture<'static, ()> + Send>> {
        let sig_sub_config = RpcSignatureSubscribeConfig {
            commitment: Some(commitment),
            enable_received_notification: None,
        };

        let (mut sig_sub, sig_sub_unsubscriber) = client
            .signature_subscribe(&tx_sig, Some(sig_sub_config))
            .await?;

        sig_sub.next().await;
        Ok(sig_sub_unsubscriber)
    }

    /// Db writer worker
    async fn write_tx(rx: Receiver<Transaction>, db_pool: DbPool) -> anyhow::Result<()> {
        loop {
            let res_ = rx.recv_async().await;
            if res_.is_err() || rx.is_disconnected() {
                // All senders have been dropped - regular exit
                break;
            }

            let tx = res_.unwrap();
            let db_pool = db_pool.clone();

            let db_insert = tokio::task::spawn_blocking(move || {
                let mut conn = db_pool.get().map_err(|e| anyhow!(e))?;
                actions::insert_new_transaction(&mut conn, tx)
            })
            .await;

            match db_insert {
                Ok(Err(_e)) => {
                    warn!("Could not insert tx into db, error: {}", _e);
                }
                Err(e) => {
                    error!("Unable to join db insert task, error={}", e);
                }
                Ok(Ok(_tx)) => {
                    info!("Insert new transactions: {:?}", _tx);
                }
            }
        }

        Ok(())
    }
}

/// Extract relevant data in order to build a Transaction struct
fn process_transaction(
    tx_sig: &Signature,
    tx_data: EncodedConfirmedTransactionWithStatusMeta,
) -> (Transaction, Vec<String>) {
    let transaction = tx_data.transaction;
    // let transaction_version = transaction.version;
    // let transaction_fee = transaction.meta.unwrap().fee;
    let mut accounts = vec![];
    if let EncodedTransaction::Json(ui_tx) = transaction.transaction {
        // let tx_senders = ui_tx.signatures;
        if let UiMessage::Parsed(ui_msg) = ui_tx.message {
            accounts.extend(
                ui_msg
                    .account_keys
                    .into_iter()
                    .map(|account| account.pubkey),
            );
        }
    }

    #[allow(deprecated)]
    let block_ts = chrono::NaiveDateTime::from_timestamp(tx_data.block_time.unwrap_or_default(), 0);
    let block_date = block_ts.date();

    (
        Transaction {
            id: tx_sig.to_string(),
            confirmed: true,
            block_date,
            block_ts,
        },
        accounts,
    )
}

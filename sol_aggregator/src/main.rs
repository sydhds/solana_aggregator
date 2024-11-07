use anyhow::anyhow;
use clap::Parser;
use sol_account_collector::AccountCollector;
use sol_db::initialize_db_pool;
use sol_rest_api::server::http_server;
use sol_tx_collector::TxCollector;
use solana_client::nonblocking::pubsub_client::PubsubClient;
use solana_client::nonblocking::rpc_client::RpcClient;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
// use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tracing::{
    debug,
    // info
};

#[derive(Debug, Clone, Parser)]
#[command(name = "clap-subcommand")]
pub struct Cli {
    #[arg(short = 'w', long = "ws_url", help = "Solana web socket url")]
    ws_url: url::Url,
    #[arg(short = 'r', long = "rpc_url", help = "Solana json rpc url")]
    rpc_url: url::Url,
    #[arg(short = 'd', long = "db_url", help = "Db url")]
    db_url: String,
    #[arg(long = "http_host", help = "Http host", default_value = "127.0.0.1")]
    http_host: String,
    #[arg(
        short = 'p',
        long = "http_port",
        help = "Http port",
        default_value = "8080"
    )]
    http_port: u16,
}

#[tokio::main]
async fn main() {
    // install global collector configured based on RUST_LOG env var.
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();
    debug!("cli: {:?}", cli);

    // Ctrl-C
    let task_shutdown_marker = Arc::new(AtomicBool::new(false));

    // Solana web socket client
    let ws_client_ = PubsubClient::new(cli.ws_url.as_ref())
        .await
        .unwrap_or_else(|e| panic!("Unable to connect to: {}, error: {}", cli.ws_url, e));
    let ws_client = Arc::new(ws_client_);
    let ws_client_2_ = PubsubClient::new(cli.ws_url.as_ref())
        .await
        .unwrap_or_else(|e| panic!("Unable to connect to: {}, error: {}", cli.ws_url, e));
    let ws_client2 = Arc::new(ws_client_2_);

    // Solana json rpc client
    let rpc_client_ = RpcClient::new(cli.rpc_url.to_string());
    let rpc_client = Arc::new(rpc_client_);

    // db pool
    let db_pool = initialize_db_pool(cli.db_url);

    let tx_collector = TxCollector {
        ws_client: ws_client.clone(),
        rpc_client: rpc_client.clone(),
        db_pool: db_pool.clone(),
        shutdown_marker: task_shutdown_marker.clone(),
        writer_worker_count: 2,
        process_worker_count: 16,
    };

    let account_collector = AccountCollector {
        ws_client: ws_client2.clone(),
        rpc_client: rpc_client.clone(),
        db_pool: db_pool.clone(),
        max_account_to_monitor: 100,
        get_account_delay: Duration::from_secs(1),
        unsubscribe_delay: Duration::from_millis(250),
        account_writer_count: 4,
    };
    let (tx_accounts, rx_accounts) = flume::unbounded();

    // account_collector.run(rx_accounts).await;

    // http server
    let server = http_server(db_pool.clone(), cli.http_host.clone(), cli.http_port)
        .expect("Unable to init. http server");
    let server_handle = server.handle();

    let mut set = JoinSet::new();
    // Spawn tx_collector, account_collector + Ctrl-C handler task + http server
    set.spawn(async move { server.await.map_err(|e| anyhow!(e)) });
    set.spawn(async move { tx_collector.run(tx_accounts).await } );
    set.spawn(async move { account_collector.run(rx_accounts).await });
    set.spawn(async move {
        // listen for ctrl-c
        tokio::signal::ctrl_c().await.unwrap();
        // start shutdown of tasks
        let server_stop = server_handle.stop(true);
        task_shutdown_marker.store(true, Ordering::SeqCst);
        // Drop logs & db queues to shut down process_tx && write_tx tasks
        // drop(tx_logs);
        // drop(rx_logs);
        // drop(tx_db);
        // drop(rx_db);
        // await shutdown of tasks
        server_stop.await;
        Ok::<(), anyhow::Error>(())
    });

    while let Some(task_join_res) = set.join_next().await {
        if let Ok(Ok(())) = task_join_res {
            // Nothing to do here.
        } else {
            debug!("Exiting task: {:?}", task_join_res);
        }
    }
}

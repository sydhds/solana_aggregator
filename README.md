# Solana aggregator

A Solana Blockchain Data Aggregator:
* Process account changes 
* Process transactions 
* Diesel ORM (for database handling)
* Actix web server (REST API)

# Setup

## Install SQLite

### on OpenSUSE
sudo zypper install sqlite3-devel libsqlite3-0 sqlite3

### on Ubuntu
sudo apt-get install libsqlite3-dev sqlite3

### on Fedora
sudo dnf install libsqlite3x-devel sqlite3x

### on macOS (using homebrew)
brew install sqlite3

## Initialize SQLite Db

* cargo install diesel_cli --no-default-features --features sqlite
* echo "DATABASE_URL=test.db" > .env
* diesel setup
* diesel migration run

## Run

RUST_LOG=info cargo run -- -w wss://api.testnet.solana.com -r https://api.testnet.solana.com -d ./test.db

## Http server url

* Print the account (ordered by coins)
    * http://127.0.0.1:8080/accounts/
* Print the account details
    * http://127.0.0.1:8080/accounts/?id=__ACCOUNT_ID__

* Print the last 5 transactions:
    * http://127.0.0.1:8080/transactions/
* Get the transaction details (given a transaction id):
    * http://127.0.0.1:8080/transactions/?id=__TRANSACTION_ID__
* Filter transactions by date:
    * http://127.0.0.1:8080/transactions/?date=2024-09-12

## Unit tests

* cargo test -- --nocapture

# Design notes

## Tx collector

* Wait for logs_subscribe Solana WS endpoint
* Use N tasks (can be configured) to process logs
* Use N tasks (can be configured) to write to DB
* Send accounts to Account collector

## Account collector

* Wait for Tx collector to send new accounts (as String)
    * Query account info and spawn a monitoring task
* Use LRU map to restrict the number of monitored account & cancel the ended monitoring task
* Use N tasks (can be configured) to write to DB

## Design performance notes

* In order to achieve decent performance, a Postgres or Mysql DB should be used. By using the Diesel orm, it should be easy.
* Selected crates for their performance:
    * flume
    * schnellLRU

## TODO

* Fixe error when a transaction is already in the DB
* More tokio::timeout
* Use bounded channel (where applicable)
* [Perf] Batch write/update to DB
    * Account batch updates would really help the aggregator
* Write more unit tests
    * Mock PubSubClient: define a trait, impl a Mock using this trait and use a Generic + Box<dyn PubSubTrait>
* Stress test the http server of the aggregator using "Apache ab" or Locust (Python tool)

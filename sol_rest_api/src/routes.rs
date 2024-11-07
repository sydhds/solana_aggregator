use actix_web::{error, get, web, HttpRequest, HttpResponse, Responder};
use chrono::NaiveDate;
use serde::Deserialize;
use sol_db::models::{Account, Transaction};
use sol_db::{actions, actions_account, DbPool};
use tracing::debug;

/// Request extract to handle ?id= or ?date=
#[derive(Debug, Deserialize, Clone)]
struct TransactionsRequest {
    id: Option<String>,
    date: Option<NaiveDate>,
}

/// Page handler: / (aka index page)
pub async fn index(req: HttpRequest) -> &'static str {
    debug!("REQ: {req:?}");
    "Solana dashboard ggregator"
}

#[get("/transactions/")]
pub async fn get_transaction(
    pool: web::Data<DbPool>,
    info: web::Query<TransactionsRequest>,
) -> actix_web::Result<impl Responder> {
    // dbg!("object = {:?}", info.clone().into_inner());

    let txs: Vec<Transaction> = match info {
        ref i if i.id.is_some() => {
            let tx_id = i.id.clone().unwrap();

            // use web::block to offload blocking Diesel queries without blocking server thread
            let tx = web::block(move || {
                // note that obtaining a connection from the pool is also potentially blocking
                let mut conn = pool.get()?;
                actions::find_tx_by_id(&mut conn, &tx_id)
            })
            .await?
            // map diesel query errors to a 500 error response
            .map_err(error::ErrorInternalServerError)?;

            tx.map(|tx| vec![tx]).unwrap_or_default()
        }
        ref i if i.date.is_some() => {
            let req_date = i.date.unwrap();
            web::block(move || {
                let mut conn = pool.get()?;
                actions::find_txs_by_date(&mut conn, req_date, 1000)
            })
            .await?
            .map_err(error::ErrorInternalServerError)?
        }
        _ => {
            // No parameter - return the last five tx
            web::block(move || {
                let mut conn = pool.get()?;
                actions::find_latest_txs(&mut conn, 5)
            })
            .await?
            .map_err(error::ErrorInternalServerError)?
        }
    };

    if txs.is_empty() {
        Ok(HttpResponse::NotFound().body(format!("No transactions found with query: {:?}", info)))
    } else {
        Ok(HttpResponse::Ok().json(txs))
    }
}

/// Request extract to handle ?id= or ?date=
#[derive(Debug, Deserialize, Clone)]
struct AccountsRequest {
    id: Option<String>,
}

#[get("/accounts/")]
pub async fn get_account(
    pool: web::Data<DbPool>,
    info: web::Query<AccountsRequest>,
) -> actix_web::Result<impl Responder> {
    // dbg!("object = {:?}", info.clone().into_inner());

    let acc: Vec<Account> = match info {
        ref i if i.id.is_some() => {
            let acc_id = i.id.clone().unwrap();

            // use web::block to offload blocking Diesel queries without blocking server thread
            let tx = web::block(move || {
                // note that obtaining a connection from the pool is also potentially blocking
                let mut conn = pool.get()?;
                actions_account::find_account_by_id(&mut conn, &acc_id)
            })
            .await?
            // map diesel query errors to a 500 error response
            .map_err(error::ErrorInternalServerError)?;

            tx.map(|tx| vec![tx]).unwrap_or_default()
        }
        _ => {
            // No parameter - return the last five tx
            web::block(move || {
                let mut conn = pool.get()?;
                actions_account::find_latest_accounts(&mut conn, 5)
            })
            .await?
            .map_err(error::ErrorInternalServerError)?
        }
    };

    if acc.is_empty() {
        Ok(HttpResponse::NotFound().body(format!("No accounts found with query: {:?}", info)))
    } else {
        Ok(HttpResponse::Ok().json(acc))
    }
}
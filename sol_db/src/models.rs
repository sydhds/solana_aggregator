use diesel::internal::derives::multiconnection::chrono;
use diesel::prelude::*;
use serde::{Deserialize, Serialize};

use crate::schema::{accounts, transactions};

/// Tx details
#[derive(Debug, Clone, Serialize, Deserialize, Queryable, Insertable)]
#[diesel(table_name = transactions)]
pub struct Transaction {
    pub id: String, // TODO: no String?
    pub confirmed: bool,
    pub block_date: chrono::NaiveDate,
    pub block_ts: chrono::NaiveDateTime,
}

/// Account details
#[derive(Debug, Clone, Serialize, Deserialize, Queryable, Insertable)]
#[diesel(table_name = accounts)]
pub struct Account {
    pub id: String,       // TODO: no String?
    pub lamports: String, // BigInt ~ i64, but no u64 support in sqlite?
    pub owner: String,
}

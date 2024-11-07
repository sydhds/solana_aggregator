use chrono::NaiveDate;
use diesel::prelude::*;

use crate::models;

/// Run query using Diesel to find tx by id and return it.
pub fn find_tx_by_id(
    conn: &mut SqliteConnection,
    tx_id: &String,
) -> anyhow::Result<Option<models::Transaction>> {
    use crate::schema::transactions::dsl::*;

    let tx = transactions
        .filter(id.eq(tx_id))
        .first::<models::Transaction>(conn)
        .optional()?;

    Ok(tx)
}

/// Run query using Diesel to find txs by date and return them.
pub fn find_txs_by_date(
    conn: &mut SqliteConnection,
    date: NaiveDate,
    limit: i64,
) -> anyhow::Result<Vec<models::Transaction>> {
    use crate::schema::transactions::dsl::*;
    let txs = transactions
        .filter(block_date.eq(date))
        .limit(limit)
        .load(conn)?;

    Ok(txs)
}

/// Run query using Diesel to find latest txs (with limit) and return them.
pub fn find_latest_txs(
    conn: &mut SqliteConnection,
    limit: i64,
) -> anyhow::Result<Vec<models::Transaction>> {
    use crate::schema::transactions::dsl::*;
    let txs = transactions
        .limit(limit)
        .order(block_ts.desc())
        .load(conn)?;
    Ok(txs)
}

/// Run query using Diesel to insert a new database row and return the result.
pub fn insert_new_transaction(
    conn: &mut SqliteConnection,
    tx: models::Transaction,
) -> anyhow::Result<models::Transaction> {
    // It is common when using Diesel with Actix Web to import schema-related
    // modules inside a function's scope (rather than the normal module's scope)
    // to prevent import collisions and namespace pollution.
    use crate::schema::transactions::dsl::*;
    diesel::insert_into(transactions)
        .values(&tx)
        .execute(conn)?;

    Ok(tx)
}

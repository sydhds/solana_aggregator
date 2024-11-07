use diesel::prelude::*;

use crate::models;

/// Run query using Diesel to find account by id and return it.
pub fn find_account_by_id(
    conn: &mut SqliteConnection,
    account_id: &String,
) -> anyhow::Result<Option<models::Account>> {
    use crate::schema::accounts::dsl::*;

    let acc = accounts
        .filter(id.eq(account_id))
        .first::<models::Account>(conn)
        .optional()?;

    Ok(acc)
}

/// Run query using Diesel to find latest accounts (with limit) and return them.
pub fn find_latest_accounts(
    conn: &mut SqliteConnection,
    limit: i64,
) -> anyhow::Result<Vec<models::Account>> {
    use crate::schema::accounts::dsl::*;
    let txs = accounts.limit(limit).order(lamports.asc()).load(conn)?;
    Ok(txs)
}

/// Run query using Diesel to insert a new database row and return the result.
pub fn insert_new_account(
    conn: &mut SqliteConnection,
    acc: models::Account,
) -> anyhow::Result<models::Account> {
    // It is common when using Diesel with Actix Web to import schema-related
    // modules inside a function's scope (rather than the normal module's scope)
    // to prevent import collisions and namespace pollution.
    use crate::schema::accounts::dsl::*;
    diesel::insert_into(accounts).values(&acc).execute(conn)?;

    Ok(acc)
}

/// Run query using Diesel to update a new database row and return the result.
pub fn update_account(
    conn: &mut SqliteConnection,
    acc: models::Account,
) -> anyhow::Result<models::Account> {
    // It is common when using Diesel with Actix Web to import schema-related
    // modules inside a function's scope (rather than the normal module's scope)
    // to prevent import collisions and namespace pollution.
    use crate::schema::accounts::dsl::*;

    let acc_ = acc.clone();
    diesel::update(accounts)
        .filter(id.eq(acc_.id))
        .set((lamports.eq(acc_.lamports), owner.eq(acc_.owner.clone())))
        .execute(conn)?;

    Ok(acc)
}

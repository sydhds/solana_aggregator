// @generated automatically by Diesel CLI.

diesel::table! {
    accounts (id) {
        id -> Text,
        lamports -> Text,
        owner -> Text,
    }
}

diesel::table! {
    transactions (id) {
        id -> Text,
        confirmed -> Bool,
        block_date -> Date,
        block_ts -> Timestamp,
    }
}

diesel::allow_tables_to_appear_in_same_query!(
    accounts,
    transactions,
);

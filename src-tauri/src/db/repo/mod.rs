//! Plain-SQL repositories, one per entity. No ORM: every statement is written out. Every write
//! takes a `CommandRecord` so the audit row lands in the same transaction.

pub mod account;
pub mod batch;
pub mod category;
pub mod ledger;
pub mod link;
pub mod rule;
pub mod saved_view;
pub mod txn;
pub mod venture;

use rusqlite::types::Value;

/// Build a `?N` placeholder list for `IN (...)` clauses.
pub fn placeholders(start: usize, count: usize) -> String {
    (0..count)
        .map(|i| format!("?{}", start + i))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Push the ids of an `IN` clause onto a parameter vector.
pub fn push_ids(params: &mut Vec<Value>, ids: &[i64]) {
    for id in ids {
        params.push(Value::Integer(*id));
    }
}

/// For `Option<Option<T>>` patch fields: a missing key means "leave it", JSON `null` means
/// "clear it", a value means "set it". Plain serde would read `null` as "leave it".
pub fn double_option<'de, T, D>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    T: serde::Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    serde::Deserialize::deserialize(d).map(Some)
}

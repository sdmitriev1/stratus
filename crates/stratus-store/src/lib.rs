pub mod schema;
pub mod store;
pub mod watch;

pub use store::Store;
pub use watch::{EventType, WatchEvent, WatchableStore};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Database(#[from] redb::DatabaseError),

    #[error("transaction error: {0}")]
    Transaction(Box<redb::TransactionError>),

    #[error("table error: {0}")]
    Table(#[from] redb::TableError),

    #[error("storage error: {0}")]
    Storage(#[from] redb::StorageError),

    #[error("commit error: {0}")]
    Commit(#[from] redb::CommitError),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("unknown resource kind: {0}")]
    UnknownKind(String),

    #[error("schema version mismatch: expected {expected}, found {found}")]
    SchemaMismatch { expected: u32, found: u32 },
}

impl From<redb::TransactionError> for StoreError {
    fn from(e: redb::TransactionError) -> Self {
        StoreError::Transaction(Box::new(e))
    }
}

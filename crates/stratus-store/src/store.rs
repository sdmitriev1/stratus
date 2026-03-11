use std::path::Path;

use redb::ReadableTable;
use stratus_resources::Resource;
use tracing::debug;

use crate::schema::{
    ALL_RESOURCE_TABLES, CHANGELOG, CONFIG, CONFIG_KEY_REVISION, CONFIG_KEY_SCHEMA_VERSION,
    CURRENT_SCHEMA_VERSION,
};
use crate::{StoreError, schema};

#[derive(Debug)]
pub struct Store {
    db: redb::Database,
}

impl Store {
    /// Open (or create) a store at the given path.
    ///
    /// Creates all tables if they don't exist and validates the schema version.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let db = redb::Database::create(path.as_ref()).map_err(StoreError::Database)?;

        // Create all tables and validate schema version in a single write transaction.
        let txn = db.begin_write()?;

        for table_def in &ALL_RESOURCE_TABLES {
            txn.open_table(*table_def).map_err(StoreError::Table)?;
        }
        txn.open_table(CHANGELOG).map_err(StoreError::Table)?;

        {
            let mut config_table = txn.open_table(CONFIG).map_err(StoreError::Table)?;

            // Check/set schema version.
            let existing_version = config_table
                .get(CONFIG_KEY_SCHEMA_VERSION)
                .map_err(StoreError::Storage)?
                .map(|v| serde_json::from_slice::<u32>(v.value()))
                .transpose()?;

            match existing_version {
                Some(found) => {
                    if found != CURRENT_SCHEMA_VERSION {
                        return Err(StoreError::SchemaMismatch {
                            expected: CURRENT_SCHEMA_VERSION,
                            found,
                        });
                    }
                }
                None => {
                    let version_bytes = serde_json::to_vec(&CURRENT_SCHEMA_VERSION)?;
                    config_table
                        .insert(CONFIG_KEY_SCHEMA_VERSION, version_bytes.as_slice())
                        .map_err(StoreError::Storage)?;
                }
            }

            // Initialize revision to 0 if not present.
            if config_table
                .get(CONFIG_KEY_REVISION)
                .map_err(StoreError::Storage)?
                .is_none()
            {
                let zero_bytes = serde_json::to_vec(&0u64)?;
                config_table
                    .insert(CONFIG_KEY_REVISION, zero_bytes.as_slice())
                    .map_err(StoreError::Storage)?;
            }
        }

        txn.commit().map_err(StoreError::Commit)?;

        debug!(path = %path.as_ref().display(), "store opened");
        Ok(Self { db })
    }

    /// Insert or update a resource. Returns the previous value if it existed.
    pub fn put(&self, resource: &Resource) -> Result<Option<Resource>, StoreError> {
        let kind = resource.kind_str();
        let name = resource.name();
        let value = serde_json::to_vec(resource)?;

        let txn = self.db.begin_write()?;
        let old = {
            let mut table = schema::open_table_for_kind(&txn, kind)?;
            let old = table
                .get(name)
                .map_err(StoreError::Storage)?
                .map(|v| serde_json::from_slice::<Resource>(v.value()))
                .transpose()?;
            table
                .insert(name, value.as_slice())
                .map_err(StoreError::Storage)?;
            old
        };
        txn.commit().map_err(StoreError::Commit)?;

        Ok(old)
    }

    /// Get a resource by kind and name.
    pub fn get(&self, kind: &str, name: &str) -> Result<Option<Resource>, StoreError> {
        let txn = self.db.begin_read()?;
        let table = schema::open_read_table_for_kind(&txn, kind)?;
        match table.get(name).map_err(StoreError::Storage)? {
            Some(value) => Ok(Some(serde_json::from_slice(value.value())?)),
            None => Ok(None),
        }
    }

    /// Delete a resource by kind and name. Returns the deleted value if it existed.
    pub fn delete(&self, kind: &str, name: &str) -> Result<Option<Resource>, StoreError> {
        let txn = self.db.begin_write()?;
        let old = {
            let mut table = schema::open_table_for_kind(&txn, kind)?;
            table
                .remove(name)
                .map_err(StoreError::Storage)?
                .map(|v| serde_json::from_slice::<Resource>(v.value()))
                .transpose()?
        };
        txn.commit().map_err(StoreError::Commit)?;

        Ok(old)
    }

    /// List all resources of a given kind.
    pub fn list(&self, kind: &str) -> Result<Vec<Resource>, StoreError> {
        let txn = self.db.begin_read()?;
        let table = schema::open_read_table_for_kind(&txn, kind)?;
        let mut results = Vec::new();
        for entry in table.iter().map_err(StoreError::Storage)? {
            let (_, value) = entry.map_err(StoreError::Storage)?;
            let resource: Resource = serde_json::from_slice(value.value())?;
            results.push(resource);
        }
        Ok(results)
    }

    /// Access the underlying database (for WatchableStore).
    pub(crate) fn db(&self) -> &redb::Database {
        &self.db
    }
}

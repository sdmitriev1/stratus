use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use redb::ReadableTable;
use serde::{Deserialize, Serialize};
use stratus_resources::Resource;
use tokio::sync::{broadcast, mpsc};
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, warn};

use crate::schema::{CHANGELOG, CONFIG, CONFIG_KEY_REVISION};
use crate::{Store, StoreError, schema};

/// Event type for watch notifications.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventType {
    Put,
    Delete,
}

/// A watch event emitted when a resource is created, updated, or deleted.
#[derive(Debug, Clone)]
pub struct WatchEvent {
    pub revision: u64,
    pub key: String,
    pub event_type: EventType,
    pub resource: Option<Resource>,
}

/// Changelog entry stored in the database.
#[derive(Debug, Serialize, Deserialize)]
struct ChangelogEntry {
    event_type: EventType,
    resource: Option<Resource>,
    timestamp: u64,
}

const BROADCAST_CAPACITY: usize = 256;

/// A store with revision tracking, changelog, and watch semantics.
pub struct WatchableStore {
    store: Store,
    revision: AtomicU64,
    tx: broadcast::Sender<WatchEvent>,
}

impl WatchableStore {
    /// Open (or create) a watchable store at the given path.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let store = Store::open(path)?;

        // Load current revision from the config table.
        let txn = store.db().begin_read()?;
        let config = txn.open_table(CONFIG).map_err(StoreError::Table)?;
        let revision = match config
            .get(CONFIG_KEY_REVISION)
            .map_err(StoreError::Storage)?
        {
            Some(v) => serde_json::from_slice::<u64>(v.value())?,
            None => 0,
        };

        let (tx, _) = broadcast::channel(BROADCAST_CAPACITY);

        Ok(Self {
            store,
            revision: AtomicU64::new(revision),
            tx,
        })
    }

    /// Insert or update a resource, incrementing the revision and recording in changelog.
    pub fn put(&self, resource: &Resource) -> Result<(u64, Option<Resource>), StoreError> {
        let kind = resource.kind_str();
        let name = resource.name();
        let key = format!("{}/{}", kind, name);
        let value = serde_json::to_vec(resource)?;

        let new_rev = self.revision.fetch_add(1, Ordering::SeqCst) + 1;

        let txn = self.store.db().begin_write()?;
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

        // Write revision to config.
        {
            let mut config = txn.open_table(CONFIG).map_err(StoreError::Table)?;
            let rev_bytes = serde_json::to_vec(&new_rev)?;
            config
                .insert(CONFIG_KEY_REVISION, rev_bytes.as_slice())
                .map_err(StoreError::Storage)?;
        }

        // Write changelog entry.
        {
            let entry = ChangelogEntry {
                event_type: EventType::Put,
                resource: Some(resource.clone()),
                timestamp: now_epoch_secs(),
            };
            let entry_bytes = serde_json::to_vec(&entry)?;
            let mut changelog = txn.open_table(CHANGELOG).map_err(StoreError::Table)?;
            changelog
                .insert((new_rev, key.as_str()), entry_bytes.as_slice())
                .map_err(StoreError::Storage)?;
        }

        txn.commit().map_err(StoreError::Commit)?;

        // Broadcast (non-fatal if no receivers).
        let event = WatchEvent {
            revision: new_rev,
            key,
            event_type: EventType::Put,
            resource: Some(resource.clone()),
        };
        let _ = self.tx.send(event);

        debug!(revision = new_rev, kind, name, "put resource");
        Ok((new_rev, old))
    }

    /// Delete a resource, incrementing the revision and recording in changelog.
    pub fn delete(&self, kind: &str, name: &str) -> Result<(u64, Option<Resource>), StoreError> {
        let key = format!("{}/{}", kind, name);

        let new_rev = self.revision.fetch_add(1, Ordering::SeqCst) + 1;

        let txn = self.store.db().begin_write()?;
        let old = {
            let mut table = schema::open_table_for_kind(&txn, kind)?;
            table
                .remove(name)
                .map_err(StoreError::Storage)?
                .map(|v| serde_json::from_slice::<Resource>(v.value()))
                .transpose()?
        };

        // Write revision to config.
        {
            let mut config = txn.open_table(CONFIG).map_err(StoreError::Table)?;
            let rev_bytes = serde_json::to_vec(&new_rev)?;
            config
                .insert(CONFIG_KEY_REVISION, rev_bytes.as_slice())
                .map_err(StoreError::Storage)?;
        }

        // Write changelog entry.
        {
            let entry = ChangelogEntry {
                event_type: EventType::Delete,
                resource: None,
                timestamp: now_epoch_secs(),
            };
            let entry_bytes = serde_json::to_vec(&entry)?;
            let mut changelog = txn.open_table(CHANGELOG).map_err(StoreError::Table)?;
            changelog
                .insert((new_rev, key.as_str()), entry_bytes.as_slice())
                .map_err(StoreError::Storage)?;
        }

        txn.commit().map_err(StoreError::Commit)?;

        let event = WatchEvent {
            revision: new_rev,
            key,
            event_type: EventType::Delete,
            resource: None,
        };
        let _ = self.tx.send(event);

        debug!(revision = new_rev, kind, name, "deleted resource");
        Ok((new_rev, old))
    }

    /// Get a resource by kind and name (delegates to inner store).
    pub fn get(&self, kind: &str, name: &str) -> Result<Option<Resource>, StoreError> {
        self.store.get(kind, name)
    }

    /// List all resources of a given kind (delegates to inner store).
    pub fn list(&self, kind: &str) -> Result<Vec<Resource>, StoreError> {
        self.store.list(kind)
    }

    /// Current revision number.
    pub fn revision(&self) -> u64 {
        self.revision.load(Ordering::SeqCst)
    }

    /// Watch for events matching a key prefix, optionally replaying from a given revision.
    ///
    /// Returns a stream of `WatchEvent`s. The stream replays historical events from the
    /// changelog first, then delivers live events.
    pub fn watch(
        &self,
        prefix: &str,
        from_revision: u64,
    ) -> Result<ReceiverStream<WatchEvent>, StoreError> {
        // Subscribe to broadcast BEFORE reading historical events to avoid gaps.
        let mut broadcast_rx = self.tx.subscribe();

        // Read historical events from changelog.
        let mut historical = Vec::new();
        let txn = self.store.db().begin_read()?;
        let changelog = txn.open_table(CHANGELOG).map_err(StoreError::Table)?;

        let range_start = (from_revision, "");
        for entry in changelog
            .range(range_start..)
            .map_err(StoreError::Storage)?
        {
            let (k, v) = entry.map_err(StoreError::Storage)?;
            let (rev, key_str) = k.value();
            if key_str.starts_with(prefix) {
                let changelog_entry: ChangelogEntry = serde_json::from_slice(v.value())?;
                historical.push(WatchEvent {
                    revision: rev,
                    key: key_str.to_string(),
                    event_type: changelog_entry.event_type,
                    resource: changelog_entry.resource,
                });
            }
        }
        drop(changelog);
        drop(txn);

        let last_historical_rev = historical.last().map(|e| e.revision).unwrap_or(0);

        let (mpsc_tx, mpsc_rx) = mpsc::channel(256);
        let prefix_owned = prefix.to_string();

        tokio::spawn(async move {
            // Send historical events.
            for event in historical {
                if mpsc_tx.send(event).await.is_err() {
                    return;
                }
            }

            // Forward live broadcast events.
            loop {
                match broadcast_rx.recv().await {
                    Ok(event) => {
                        // Skip events already covered by historical replay.
                        if event.revision <= last_historical_rev {
                            continue;
                        }
                        if event.key.starts_with(&prefix_owned)
                            && mpsc_tx.send(event).await.is_err()
                        {
                            return;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!(skipped = n, "watch broadcast lagged");
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        return;
                    }
                }
            }
        });

        Ok(ReceiverStream::new(mpsc_rx))
    }

    /// Compact the changelog, removing entries older than `max_age`.
    /// Returns the number of entries removed.
    pub fn compact(&self, max_age: Duration) -> Result<u64, StoreError> {
        let cutoff = now_epoch_secs().saturating_sub(max_age.as_secs());

        let txn = self.store.db().begin_write()?;
        let mut removed = 0u64;

        {
            let mut changelog = txn.open_table(CHANGELOG).map_err(StoreError::Table)?;

            // Collect keys to remove (can't mutate while iterating).
            let mut to_remove = Vec::new();
            for entry in changelog.iter().map_err(StoreError::Storage)? {
                let (k, v) = entry.map_err(StoreError::Storage)?;
                let changelog_entry: ChangelogEntry = serde_json::from_slice(v.value())?;
                if changelog_entry.timestamp <= cutoff {
                    let (rev, key_str) = k.value();
                    to_remove.push((rev, key_str.to_string()));
                }
            }

            for (rev, key) in &to_remove {
                changelog
                    .remove((*rev, key.as_str()))
                    .map_err(StoreError::Storage)?;
                removed += 1;
            }
        }

        txn.commit().map_err(StoreError::Commit)?;

        debug!(removed, "changelog compacted");
        Ok(removed)
    }
}

fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

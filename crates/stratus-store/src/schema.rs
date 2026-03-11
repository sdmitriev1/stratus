use redb::TableDefinition;

use crate::StoreError;

pub const NETWORKS: TableDefinition<&str, &[u8]> = TableDefinition::new("networks");
pub const SUBNETS: TableDefinition<&str, &[u8]> = TableDefinition::new("subnets");
pub const INSTANCES: TableDefinition<&str, &[u8]> = TableDefinition::new("instances");
pub const SECURITY_GROUPS: TableDefinition<&str, &[u8]> = TableDefinition::new("security_groups");
pub const IMAGES: TableDefinition<&str, &[u8]> = TableDefinition::new("images");
pub const PORT_FORWARDS: TableDefinition<&str, &[u8]> = TableDefinition::new("port_forwards");
pub const CONFIG: TableDefinition<&str, &[u8]> = TableDefinition::new("config");
pub const CHANGELOG: TableDefinition<(u64, &str), &[u8]> = TableDefinition::new("changelog");

pub const CONFIG_KEY_REVISION: &str = "revision";
pub const CONFIG_KEY_SCHEMA_VERSION: &str = "schema_version";
pub const CURRENT_SCHEMA_VERSION: u32 = 1;

/// All resource tables for initialization.
pub const ALL_RESOURCE_TABLES: [TableDefinition<&str, &[u8]>; 6] = [
    NETWORKS,
    SUBNETS,
    INSTANCES,
    SECURITY_GROUPS,
    IMAGES,
    PORT_FORWARDS,
];

/// Opens the appropriate table for a given kind within a write transaction.
pub fn open_table_for_kind<'txn>(
    txn: &'txn redb::WriteTransaction,
    kind: &str,
) -> Result<redb::Table<'txn, &'static str, &'static [u8]>, StoreError> {
    let def = table_def_for_kind(kind)?;
    txn.open_table(def).map_err(StoreError::Table)
}

/// Opens the appropriate table for a given kind within a read transaction.
pub fn open_read_table_for_kind(
    txn: &redb::ReadTransaction,
    kind: &str,
) -> Result<redb::ReadOnlyTable<&'static str, &'static [u8]>, StoreError> {
    let def = table_def_for_kind(kind)?;
    txn.open_table(def).map_err(StoreError::Table)
}

fn table_def_for_kind(
    kind: &str,
) -> Result<TableDefinition<'static, &'static str, &'static [u8]>, StoreError> {
    match kind {
        "Network" => Ok(NETWORKS),
        "Subnet" => Ok(SUBNETS),
        "Instance" => Ok(INSTANCES),
        "SecurityGroup" => Ok(SECURITY_GROUPS),
        "Image" => Ok(IMAGES),
        "PortForward" => Ok(PORT_FORWARDS),
        _ => Err(StoreError::UnknownKind(kind.to_string())),
    }
}

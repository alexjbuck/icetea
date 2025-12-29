//! Table metadata operations and snapshot handling

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use iceberg::table::Table;
use std::collections::HashMap;

/// Simplified table metadata for display
#[derive(Debug, Clone)]
pub struct TableMetadata {
    pub location: String,
    pub schema: SchemaInfo,
    pub partition_spec: Vec<String>,
    pub sort_order: Vec<String>,
    pub properties: HashMap<String, String>,
    pub current_snapshot_id: Option<i64>,
    pub snapshots: Vec<SnapshotInfo>,
}

/// Schema information
#[derive(Debug, Clone)]
pub struct SchemaInfo {
    pub fields: Vec<FieldInfo>,
}

/// Field information
#[derive(Debug, Clone)]
pub struct FieldInfo {
    pub id: i32,
    pub name: String,
    pub field_type: String,
    pub required: bool,
}

/// Snapshot information
#[derive(Debug, Clone)]
pub struct SnapshotInfo {
    pub snapshot_id: i64,
    pub parent_snapshot_id: Option<i64>,
    pub timestamp_ms: i64,
    pub operation: String,
    pub summary: HashMap<String, String>,
}

impl TableMetadata {
    /// Extract metadata from an Iceberg table
    pub fn from_table(table: &Table) -> Result<Self> {
        let metadata = table.metadata();

        // Extract schema
        let _schema = metadata.current_schema();
        let schema_info = SchemaInfo {
            fields: Vec::new(), // TODO: Fix schema field extraction once API is clear
        };

        // Extract partition spec
        let partition_spec = metadata
            .default_partition_spec()
            .map(|spec| {
                spec.fields()
                    .iter()
                    .map(|field| format!("{:?}", field))
                    .collect()
            })
            .unwrap_or_default();

        // Extract sort order
        let sort_order = Vec::new(); // TODO: Fix sort order extraction once API is clear

        // Extract properties
        let properties = metadata.properties().clone();

        // Extract snapshots
        let current_snapshot_id = metadata.current_snapshot().map(|s| s.snapshot_id());

        let snapshots: Vec<SnapshotInfo> = metadata
            .snapshots()
            .map(|snapshot| {
                let summary_map: HashMap<String, String> = snapshot
                    .summary()
                    .other
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();

                SnapshotInfo {
                    snapshot_id: snapshot.snapshot_id(),
                    parent_snapshot_id: snapshot.parent_snapshot_id(),
                    timestamp_ms: snapshot.timestamp().timestamp_millis(),
                    operation: summary_map
                        .get("operation")
                        .cloned()
                        .unwrap_or_else(|| "unknown".to_string()),
                    summary: summary_map,
                }
            })
            .collect();

        Ok(Self {
            location: metadata.location().to_string(),
            schema: schema_info,
            partition_spec,
            sort_order,
            properties,
            current_snapshot_id,
            snapshots,
        })
    }

    /// Get snapshot history as a linear chain
    pub fn snapshot_chain(&self) -> Vec<&SnapshotInfo> {
        let mut chain = Vec::new();
        let mut current_id = self.current_snapshot_id;

        while let Some(id) = current_id {
            if let Some(snapshot) = self.snapshots.iter().find(|s| s.snapshot_id == id) {
                chain.push(snapshot);
                current_id = snapshot.parent_snapshot_id;
            } else {
                break;
            }
        }

        chain
    }
}

impl SnapshotInfo {
    /// Get timestamp as DateTime
    pub fn timestamp(&self) -> DateTime<Utc> {
        DateTime::from_timestamp_millis(self.timestamp_ms).unwrap_or_default()
    }

    /// Get summary value
    pub fn summary_value(&self, key: &str) -> Option<&str> {
        self.summary.get(key).map(|s| s.as_str())
    }

    /// Get added files count
    pub fn added_files_count(&self) -> Option<i64> {
        self.summary_value("added-files-count")
            .and_then(|s| s.parse().ok())
    }

    /// Get deleted files count
    pub fn deleted_files_count(&self) -> Option<i64> {
        self.summary_value("deleted-files-count")
            .and_then(|s| s.parse().ok())
    }

    /// Get total records
    pub fn total_records(&self) -> Option<i64> {
        self.summary_value("total-records")
            .and_then(|s| s.parse().ok())
    }
}

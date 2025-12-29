//! Iceberg catalog and table operations

pub mod catalog;
pub mod metadata;
pub mod query;

pub use catalog::CatalogManager;
pub use metadata::{TableMetadata, SnapshotInfo};

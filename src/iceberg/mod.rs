//! Iceberg catalog and table operations

pub mod catalog;
pub mod metadata;
pub mod query;
pub mod table_provider;

pub use catalog::CatalogManager;
pub use metadata::{TableMetadata, SnapshotInfo};
pub use table_provider::IcebergTableProvider;

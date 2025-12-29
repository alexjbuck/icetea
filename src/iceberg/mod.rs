//! Iceberg catalog and table operations

pub mod catalog;
pub mod catalog_provider;
pub mod metadata;
pub mod query;
pub mod table_provider;

pub use catalog::CatalogManager;
pub use catalog_provider::IcebergCatalogProvider;
pub use metadata::{TableMetadata, SnapshotInfo};
pub use table_provider::IcebergTableProvider;

//! Iceberg catalog and table operations

pub mod catalog;
pub mod catalog_provider;
pub mod metadata;
pub mod query;
pub mod stats;
pub mod table_provider;

pub use catalog::CatalogManager;
pub use metadata::TableMetadata;
pub use stats::{LazyStats, StatsProgress, TableStats, collect_table_stats, format_bytes};

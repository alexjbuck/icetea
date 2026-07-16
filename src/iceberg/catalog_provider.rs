//! DataFusion CatalogProvider and SchemaProvider for Iceberg catalogs

use async_trait::async_trait;
use datafusion::catalog::{CatalogProvider, SchemaProvider};
use datafusion::datasource::TableProvider;
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use iceberg::Catalog;
use std::sync::Arc;
use tracing::warn;

use crate::iceberg::table_provider::IcebergTableProvider;

/// DataFusion CatalogProvider implementation for Iceberg catalogs
#[derive(Debug)]
pub struct IcebergCatalogProvider {
    catalog: Arc<dyn Catalog>,
}

impl IcebergCatalogProvider {
    pub fn new(catalog: Arc<dyn Catalog>) -> Self {
        Self { catalog }
    }
}

impl CatalogProvider for IcebergCatalogProvider {
    fn schema_names(&self) -> Vec<String> {
        // Note: This is synchronous but Iceberg's list_namespaces is async
        // We'll need to handle this appropriately in the caller
        // For now, return empty and implement lazy loading
        Vec::new()
    }

    fn schema(&self, name: &str) -> Option<Arc<dyn SchemaProvider>> {
        // Create a schema provider for the requested namespace
        let namespace = match iceberg::NamespaceIdent::from_vec(vec![name.to_string()]) {
            Ok(ns) => ns,
            Err(e) => {
                warn!(
                    "Failed to parse namespace identifier '{}': {}. \
                     Namespace names must be non-empty and contain valid characters.",
                    name, e
                );
                return None;
            }
        };

        Some(Arc::new(IcebergSchemaProvider::new(
            self.catalog.clone(),
            namespace,
        )))
    }
}

/// DataFusion SchemaProvider implementation for Iceberg namespaces
#[derive(Debug)]
pub struct IcebergSchemaProvider {
    catalog: Arc<dyn Catalog>,
    namespace: iceberg::NamespaceIdent,
}

impl IcebergSchemaProvider {
    pub fn new(catalog: Arc<dyn Catalog>, namespace: iceberg::NamespaceIdent) -> Self {
        Self { catalog, namespace }
    }
}

#[async_trait]
impl SchemaProvider for IcebergSchemaProvider {
    fn table_names(&self) -> Vec<String> {
        // Synchronous method, but listing tables is async
        // Return empty for now - tables will be loaded lazily
        Vec::new()
    }

    async fn table(&self, name: &str) -> DataFusionResult<Option<Arc<dyn TableProvider>>> {
        // Load the table from Iceberg
        let table_ident = iceberg::TableIdent::new(
            self.namespace.clone(),
            name.to_string(),
        );

        let table = self
            .catalog
            .load_table(&table_ident)
            .await
            .map_err(|e| {
                // Preserve the error chain by wrapping the original error
                DataFusionError::External(
                    format!("Failed to load Iceberg table '{}': {:#}", name, e).into()
                )
            })?;

        let provider = IcebergTableProvider::new(Arc::new(table))
            .map_err(|e| {
                // Preserve the error chain from anyhow
                DataFusionError::External(
                    format!("Failed to create table provider for '{}': {:#}", name, e).into()
                )
            })?;

        Ok(Some(Arc::new(provider)))
    }

    fn table_exist(&self, _name: &str) -> bool {
        // Conservative: assume it might exist
        // DataFusion will call table() to verify
        true
    }
}

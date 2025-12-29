//! Catalog management for multiple Iceberg catalogs

use crate::config::CatalogConfig;
use anyhow::{Context, Result};
use iceberg::Catalog;
use std::collections::HashMap;
use std::sync::Arc;

/// Manages multiple Iceberg catalog connections
pub struct CatalogManager {
    catalogs: HashMap<String, Arc<dyn Catalog>>,
}

impl CatalogManager {
    /// Create a new catalog manager
    pub fn new() -> Self {
        Self {
            catalogs: HashMap::new(),
        }
    }

    /// Connect to a catalog using the provided configuration
    pub async fn connect(&mut self, _name: String, _config: &CatalogConfig) -> Result<()> {
        // TODO: Implement REST catalog connection
        // With iceberg 0.7, RestCatalog::new() is available but has a more complex
        // configuration API. Integration options:
        //
        // 1. Use RestCatalog directly with proper config builder
        // 2. Use datafusion_iceberg which provides table provider integration
        // 3. Create a custom wrapper that handles the configuration
        //
        // For now, this is stubbed to allow the project to compile.
        // See: https://docs.rs/iceberg-catalog-rest/0.7.0/
        // See: https://docs.rs/datafusion_iceberg/0.7.0/
        anyhow::bail!("REST catalog connection not yet fully implemented - needs proper config setup for iceberg 0.7")
    }

    /// Get a catalog by name
    pub fn get_catalog(&self, name: &str) -> Option<Arc<dyn Catalog>> {
        self.catalogs.get(name).cloned()
    }

    /// List all connected catalog names
    pub fn list_catalogs(&self) -> Vec<String> {
        self.catalogs.keys().cloned().collect()
    }

    /// List namespaces in a catalog
    pub async fn list_namespaces(&self, catalog_name: &str) -> Result<Vec<String>> {
        let catalog = self
            .get_catalog(catalog_name)
            .context("Catalog not found")?;

        let namespaces = catalog
            .list_namespaces(None)
            .await
            .context("Failed to list namespaces")?;

        Ok(namespaces
            .into_iter()
            .map(|ns| format!("{:?}", ns))
            .collect())
    }

    /// List tables in a namespace
    pub async fn list_tables(
        &self,
        catalog_name: &str,
        namespace: &str,
    ) -> Result<Vec<String>> {
        let catalog = self
            .get_catalog(catalog_name)
            .context("Catalog not found")?;

        let namespace = iceberg::NamespaceIdent::from_strs(namespace.split('.'))
            .context("Invalid namespace")?;

        let tables = catalog
            .list_tables(&namespace)
            .await
            .context("Failed to list tables")?;

        Ok(tables
            .into_iter()
            .map(|table| table.name().to_string())
            .collect())
    }

    /// Load a table
    pub async fn load_table(
        &self,
        catalog_name: &str,
        namespace: &str,
        table_name: &str,
    ) -> Result<iceberg::table::Table> {
        let catalog = self
            .get_catalog(catalog_name)
            .context("Catalog not found")?;

        let namespace = iceberg::NamespaceIdent::from_strs(namespace.split('.'))
            .context("Invalid namespace")?;

        let table_ident = iceberg::TableIdent::new(namespace, table_name.to_string());

        catalog
            .load_table(&table_ident)
            .await
            .context("Failed to load table")
    }
}

impl Default for CatalogManager {
    fn default() -> Self {
        Self::new()
    }
}

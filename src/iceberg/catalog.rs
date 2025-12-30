//! Catalog management for multiple Iceberg catalogs

use crate::config::CatalogConfig;
use crate::iceberg::catalog_provider::IcebergCatalogProvider;
use anyhow::{Context, Result};
use datafusion::catalog::CatalogProvider;
use iceberg::{Catalog, CatalogBuilder};
use iceberg_catalog_rest::RestCatalogBuilder;
use std::collections::HashMap;
use std::sync::Arc;

/// Manages multiple Iceberg catalog connections
pub struct CatalogManager {
    catalogs: HashMap<String, Arc<dyn Catalog>>,
    df_catalogs: HashMap<String, Arc<dyn CatalogProvider>>,
}

impl CatalogManager {
    /// Create a new catalog manager
    pub fn new() -> Self {
        Self {
            catalogs: HashMap::new(),
            df_catalogs: HashMap::new(),
        }
    }

    /// Connect to a catalog using the provided configuration
    pub async fn connect(&mut self, name: String, config: &CatalogConfig) -> Result<()> {
        match config.catalog_type.as_str() {
            "rest" => {
                // Build properties for REST catalog
                let mut props = HashMap::new();
                props.insert("uri".to_string(), config.uri.clone());

                if let Some(warehouse) = &config.warehouse {
                    props.insert("warehouse".to_string(), warehouse.clone());
                }

                // Add all additional properties
                for (key, value) in &config.properties {
                    props.insert(key.clone(), value.clone());
                }

                // Create REST catalog using the builder
                let builder = RestCatalogBuilder::default();
                let rest_catalog = builder
                    .load(name.clone(), props)
                    .await
                    .context("Failed to load REST catalog")?;

                let iceberg_catalog: Arc<dyn Catalog> = Arc::new(rest_catalog);

                // Create DataFusion catalog provider wrapping the Iceberg catalog
                let df_catalog_provider = Arc::new(IcebergCatalogProvider::new(iceberg_catalog.clone()));

                // Store both catalogs
                self.catalogs.insert(name.clone(), iceberg_catalog);
                self.df_catalogs.insert(name.clone(), df_catalog_provider);

                Ok(())
            }
            _ => {
                anyhow::bail!(
                    "Unsupported catalog type: {}. Currently only 'rest' is supported.",
                    config.catalog_type
                );
            }
        }
    }

    /// Get a catalog by name
    pub fn get_catalog(&self, name: &str) -> Option<Arc<dyn Catalog>> {
        self.catalogs.get(name).cloned()
    }

    /// Get a DataFusion catalog provider by name
    pub fn get_df_catalog(&self, name: &str) -> Option<Arc<dyn CatalogProvider>> {
        self.df_catalogs.get(name).cloned()
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

//! Catalog management for multiple Iceberg catalogs

use crate::config::CatalogConfig;
use crate::iceberg::catalog_provider::IcebergCatalogProvider;
use anyhow::{Context, Result};
use datafusion::catalog::CatalogProvider;
use iceberg::io::StorageFactory;
use iceberg::{Catalog, CatalogBuilder};
use iceberg_catalog_rest::RestCatalogBuilder;
use iceberg_storage_opendal::OpenDalStorageFactory;
use std::collections::HashMap;
use std::sync::Arc;

/// Manages multiple Iceberg catalog connections
pub struct CatalogManager {
    catalogs: HashMap<String, Arc<dyn Catalog>>,
    df_catalogs: HashMap<String, Arc<dyn CatalogProvider>>,
    catalog_configs: HashMap<String, HashMap<String, String>>,
}

impl CatalogManager {
    /// Create a new catalog manager
    pub fn new() -> Self {
        Self {
            catalogs: HashMap::new(),
            df_catalogs: HashMap::new(),
            catalog_configs: HashMap::new(),
        }
    }

    /// Get catalog server configuration
    pub fn get_catalog_config(&self, name: &str) -> Option<&HashMap<String, String>> {
        self.catalog_configs.get(name)
    }

    /// Remove a catalog (used when connection fails)
    pub fn remove_catalog(&mut self, name: &str) {
        self.catalogs.remove(name);
        self.df_catalogs.remove(name);
        self.catalog_configs.remove(name);
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

                // IceTea runs as a local TUI — never hang probing EC2 instance metadata
                // when S3 credentials are missing or incomplete.
                props
                    .entry("s3.disable-ec2-metadata".to_string())
                    .or_insert_with(|| "true".to_string());

                // Iceberg 0.9 requires an explicit StorageFactory for FileIO.
                let storage_factory = storage_factory_for_uri(config.warehouse.as_deref());

                // Create REST catalog using the builder
                let builder = RestCatalogBuilder::default().with_storage_factory(storage_factory);
                let rest_catalog = builder
                    .load(name.clone(), props)
                    .await
                    .context("Failed to load REST catalog")?;

                let iceberg_catalog: Arc<dyn Catalog> = Arc::new(rest_catalog);

                // Create DataFusion catalog provider wrapping the Iceberg catalog
                let df_catalog_provider = Arc::new(IcebergCatalogProvider::new(iceberg_catalog.clone()));

                // Fetch catalog configuration from the /v1/config endpoint
                // This includes defaults like S3 endpoint that the server provides
                let mut config_url = format!("{}/v1/config", config.uri);

                // Add warehouse parameter if specified
                if let Some(warehouse) = &config.warehouse {
                    config_url = format!("{}?warehouse={}", config_url, urlencoding::encode(warehouse));
                }

                let server_config = match fetch_catalog_config(&config_url, &config.properties).await {
                    Ok(cfg) => {
                        tracing::info!("Fetched catalog config for {}: {} properties", name, cfg.len());
                        for (k, v) in &cfg {
                            tracing::debug!("  Server config property: {} = {}", k, v);
                        }
                        cfg
                    }
                    Err(e) => {
                        tracing::warn!("Failed to fetch catalog config for {}: {}", name, e);
                        HashMap::new()
                    }
                };

                // Store all information
                self.catalogs.insert(name.clone(), iceberg_catalog);
                self.df_catalogs.insert(name.clone(), df_catalog_provider);
                self.catalog_configs.insert(name.clone(), server_config);

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

    /// Fetch storage config for a table by making a direct REST API call
    /// This gets the storage configuration (like S3 endpoint) that the catalog server provides
    pub async fn fetch_table_storage_config(
        &self,
        _catalog_name: &str,
        catalog_config: &CatalogConfig,
        namespace: &str,
        table_name: &str,
    ) -> Result<HashMap<String, String>> {
        // Build the load table URL
        let namespace_encoded = urlencoding::encode(namespace);
        let table_encoded = urlencoding::encode(table_name);
        let url = format!(
            "{}/v1/namespaces/{}/tables/{}",
            catalog_config.uri, namespace_encoded, table_encoded
        );

        tracing::debug!("Fetching table storage config from: {}", url);

        let client = reqwest::Client::new();
        let mut request = client.get(&url);

        // Add OAuth2 token if credentials are provided
        if let Some(credential) = catalog_config.properties.get("credential") {
            if let Some(token_url) = catalog_config.properties.get("oauth2-server-uri") {
                match get_oauth2_token(token_url, credential, catalog_config.properties.get("scope")).await {
                    Ok(token) => {
                        request = request.bearer_auth(token);
                    }
                    Err(e) => {
                        tracing::warn!("Failed to get OAuth2 token: {}", e);
                    }
                }
            }
        }

        let response = request.send().await.context("Failed to fetch table")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Failed to fetch table: {} - {}", status, body);
        }

        // Parse the LoadTableResult JSON
        #[derive(serde::Deserialize)]
        struct LoadTableResult {
            #[serde(default)]
            config: HashMap<String, String>,
        }

        let result: LoadTableResult = response.json().await.context("Failed to parse table response")?;

        tracing::debug!("Got {} storage config properties from table", result.config.len());
        for (k, v) in &result.config {
            tracing::debug!("  Table storage config: {} = {}", k, v);
        }

        Ok(result.config)
    }
}

impl Default for CatalogManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Fetch catalog configuration from the REST catalog's /v1/config endpoint
async fn fetch_catalog_config(
    config_url: &str,
    auth_properties: &HashMap<String, String>,
) -> Result<HashMap<String, String>> {
    tracing::debug!("Fetching catalog config from: {}", config_url);

    let client = reqwest::Client::new();
    let mut request = client.get(config_url);

    // Add OAuth2 token if credentials are provided
    if let Some(credential) = auth_properties.get("credential") {
        if let Some(token_url) = auth_properties.get("oauth2-server-uri") {
            tracing::debug!("Obtaining OAuth2 token from: {}", token_url);
            // Get OAuth2 token
            match get_oauth2_token(token_url, credential, auth_properties.get("scope")).await {
                Ok(token) => {
                    tracing::debug!("Got OAuth2 token, adding to request");
                    request = request.bearer_auth(token);
                }
                Err(e) => {
                    tracing::warn!("Failed to get OAuth2 token: {}", e);
                }
            }
        }
    }

    let response = request.send().await.context("Failed to fetch catalog config")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("Failed to fetch catalog config: {} - {}", status, body);
    }

    tracing::debug!("Got successful response from catalog config endpoint");

    // Parse the response JSON
    #[derive(serde::Deserialize)]
    struct ConfigResponse {
        #[serde(default)]
        defaults: HashMap<String, String>,
        #[serde(default)]
        overrides: HashMap<String, String>,
    }

    let config_response: ConfigResponse = response.json().await.context("Failed to parse config response")?;

    // Merge defaults and overrides
    let mut config = config_response.defaults;
    config.extend(config_response.overrides);

    Ok(config)
}

/// Get OAuth2 access token
async fn get_oauth2_token(token_url: &str, credential: &str, scope: Option<&String>) -> Result<String> {
    // Parse credential as client_id:client_secret
    let parts: Vec<&str> = credential.split(':').collect();
    if parts.len() != 2 {
        anyhow::bail!("Invalid credential format, expected client_id:client_secret");
    }

    let client_id = parts[0];
    let client_secret = parts[1];

    // Build form data
    let mut form_data = format!(
        "grant_type=client_credentials&client_id={}&client_secret={}",
        urlencoding::encode(client_id),
        urlencoding::encode(client_secret)
    );

    if let Some(scope_value) = scope {
        form_data.push_str(&format!("&scope={}", urlencoding::encode(scope_value)));
    }

    let client = reqwest::Client::new();
    let response = client
        .post(token_url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(form_data)
        .send()
        .await
        .context("Failed to request OAuth2 token")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("OAuth2 token request failed: {} - {}", status, body);
    }

    #[derive(serde::Deserialize)]
    struct TokenResponse {
        access_token: String,
    }

    let token_response: TokenResponse = response.json().await.context("Failed to parse token response")?;

    Ok(token_response.access_token)
}

/// Pick an OpenDAL storage factory from a warehouse/table URI scheme.
pub(crate) fn storage_factory_for_uri(uri: Option<&str>) -> Arc<dyn StorageFactory> {
    let scheme = uri
        .and_then(|u| u.split_once("://").map(|(s, _)| s))
        .unwrap_or("s3");

    match scheme {
        "file" | "fs" => Arc::new(OpenDalStorageFactory::Fs),
        "s3a" | "s3n" => Arc::new(OpenDalStorageFactory::S3 {
            configured_scheme: scheme.to_string(),
            customized_credential_load: None,
        }),
        _ => Arc::new(OpenDalStorageFactory::S3 {
            configured_scheme: "s3".to_string(),
            customized_credential_load: None,
        }),
    }
}

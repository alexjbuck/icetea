//! Configuration management using figment for layered config

use anyhow::{Context, Result};
use figment::{
    providers::{Env, Format, Serialized, Toml},
    Figment,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Catalog configurations
    pub catalogs: HashMap<String, CatalogConfig>,

    /// UI settings
    #[serde(default)]
    pub ui: UiConfig,

    /// Query settings
    #[serde(default)]
    pub query: QueryConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogConfig {
    /// Catalog type (rest, hive, glue, etc.)
    pub catalog_type: String,

    /// Connection URI or endpoint
    pub uri: String,

    /// Optional warehouse location
    pub warehouse: Option<String>,

    /// Additional properties
    #[serde(default)]
    pub properties: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    /// Default theme (light, dark)
    #[serde(default = "default_theme")]
    pub theme: String,

    /// Refresh interval in seconds
    #[serde(default = "default_refresh_interval")]
    pub refresh_interval: u64,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            refresh_interval: default_refresh_interval(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryConfig {
    /// Default query timeout in seconds
    #[serde(default = "default_query_timeout")]
    pub timeout: u64,

    /// Maximum rows to fetch
    #[serde(default = "default_max_rows")]
    pub max_rows: usize,
}

impl Default for QueryConfig {
    fn default() -> Self {
        Self {
            timeout: default_query_timeout(),
            max_rows: default_max_rows(),
        }
    }
}

fn default_theme() -> String {
    "dark".to_string()
}

fn default_refresh_interval() -> u64 {
    30
}

fn default_query_timeout() -> u64 {
    300
}

fn default_max_rows() -> usize {
    10000
}

impl Config {
    /// Load configuration from multiple sources with priority:
    /// defaults < config file < environment variables < CLI arguments
    pub fn load(config_path: Option<PathBuf>, cli_catalogs: Vec<String>) -> Result<Self> {
        let mut figment = Figment::new()
            // Start with defaults
            .merge(Serialized::defaults(Config::default_config()))
            // Add environment variables (prefixed with ICETEA_)
            .merge(Env::prefixed("ICETEA_").split("_"));

        // Add config file if provided
        if let Some(path) = config_path {
            figment = figment.merge(Toml::file(path));
        }

        // Merge CLI catalogs if provided
        if !cli_catalogs.is_empty() {
            let cli_catalog_configs = Self::parse_catalog_uris(cli_catalogs)?;
            figment = figment.merge(Serialized::defaults(("catalogs", cli_catalog_configs)));
        }

        figment
            .extract()
            .context("Failed to load configuration")
    }

    fn default_config() -> Self {
        Self {
            catalogs: HashMap::new(),
            ui: UiConfig::default(),
            query: QueryConfig::default(),
        }
    }

    fn parse_catalog_uris(uris: Vec<String>) -> Result<HashMap<String, CatalogConfig>> {
        let mut catalogs = HashMap::new();

        for (idx, uri) in uris.into_iter().enumerate() {
            // Simple URI parsing - format: name=type:uri or just uri (defaults to rest)
            let (name, catalog_type, uri) = if uri.contains('=') {
                let parts: Vec<&str> = uri.splitn(2, '=').collect();
                let name = parts[0].to_string();
                let rest = parts[1];

                if rest.contains(':') {
                    let type_uri: Vec<&str> = rest.splitn(2, ':').collect();
                    (name, type_uri[0].to_string(), type_uri[1].to_string())
                } else {
                    (name, "rest".to_string(), rest.to_string())
                }
            } else {
                (
                    format!("catalog_{}", idx),
                    "rest".to_string(),
                    uri,
                )
            };

            catalogs.insert(
                name,
                CatalogConfig {
                    catalog_type,
                    uri,
                    warehouse: None,
                    properties: HashMap::new(),
                },
            );
        }

        Ok(catalogs)
    }
}

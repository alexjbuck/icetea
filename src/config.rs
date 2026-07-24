//! Configuration management using figment for layered config
//!
//! Precedence (highest wins): CLI arguments > environment variables > config file > defaults

use anyhow::{bail, Context, Result};
use figment::{
    providers::{Env, Format, Serialized, Toml},
    Figment,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::cli::Cli;

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
    /// Load configuration with precedence: defaults < config file < env < CLI
    pub fn load(cli: &Cli) -> Result<Self> {
        let mut figment = Figment::new()
            // Start with defaults
            .merge(Serialized::defaults(Config::default_config()));

        // Load from default config locations (lower priority first)
        for path in Self::default_config_paths() {
            if path.exists() {
                figment = figment.merge(Toml::file(&path));
            }
        }

        // Explicit config file overrides default locations
        if let Some(path) = &cli.config {
            figment = figment.merge(Toml::file(path));
        }

        // Environment variables (ICETEA_ prefix, `__` separates nested keys)
        // Examples:
        //   ICETEA_UI__THEME=dark
        //   ICETEA_CATALOGS__MY_CAT__URI=http://localhost:8181
        //   ICETEA_CATALOGS__MY_CAT__PROPERTIES__TOKEN=secret
        figment = figment.merge(Env::prefixed("ICETEA_").split("__"));

        let mut config: Config = figment
            .extract()
            .context("Failed to load configuration")?;

        // CLI arguments win over everything else
        config.apply_cli_overrides(cli)?;

        Ok(config)
    }

    fn apply_cli_overrides(&mut self, cli: &Cli) -> Result<()> {
        if let Some(theme) = &cli.theme {
            self.ui.theme = theme.clone();
        }
        if let Some(interval) = cli.refresh_interval {
            self.ui.refresh_interval = interval;
        }
        if let Some(timeout) = cli.query_timeout {
            self.query.timeout = timeout;
        }
        if let Some(max_rows) = cli.max_rows {
            self.query.max_rows = max_rows;
        }

        if !cli.catalogs.is_empty() {
            let cli_catalogs = parse_catalog_uris(&cli.catalogs)?;
            for (name, catalog) in cli_catalogs {
                self.catalogs
                    .entry(name)
                    .and_modify(|existing| {
                        existing.catalog_type = catalog.catalog_type.clone();
                        existing.uri = catalog.uri.clone();
                    })
                    .or_insert(catalog);
            }
        }

        for warehouse_spec in &cli.catalog_warehouses {
            let (name, warehouse) = parse_name_value(warehouse_spec, "catalog-warehouse")?;
            let catalog = self.catalogs.get_mut(&name).with_context(|| {
                format!(
                    "Unknown catalog `{name}` in --catalog-warehouse (define it with --catalog or in config first)"
                )
            })?;
            catalog.warehouse = Some(warehouse);
        }

        for prop_spec in &cli.catalog_properties {
            let (name, key, value) = parse_catalog_property(prop_spec)?;
            let catalog = self.catalogs.get_mut(&name).with_context(|| {
                format!(
                    "Unknown catalog `{name}` in --catalog-property (define it with --catalog or in config first)"
                )
            })?;
            catalog.properties.insert(key, value);
        }

        Ok(())
    }

    /// Returns default config file paths in order of priority (lowest first)
    fn default_config_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();

        // XDG_CONFIG_HOME/icetea/config.toml or ~/.config/icetea/config.toml
        if let Some(config_dir) = std::env::var_os("XDG_CONFIG_HOME") {
            paths.push(PathBuf::from(config_dir).join("icetea/config.toml"));
        } else if let Some(home) = std::env::var_os("HOME") {
            paths.push(PathBuf::from(home).join(".config/icetea/config.toml"));
        }

        // Current directory: icetea.toml or config.toml
        paths.push(PathBuf::from("icetea.toml"));
        paths.push(PathBuf::from("config.toml"));

        paths
    }

    /// Default log path: `$XDG_STATE_HOME/icetea/icetea.log`, else `~/.local/state/icetea/icetea.log`.
    /// Falls back to `./icetea.log` when neither `XDG_STATE_HOME` nor `HOME` is set.
    pub fn default_log_path() -> PathBuf {
        resolve_log_path(
            std::env::var_os("XDG_STATE_HOME"),
            std::env::var_os("HOME"),
        )
    }

    /// Open the log file for append, creating parent directories as needed.
    ///
    /// Uses `explicit` when provided; otherwise [`default_log_path`]. If the
    /// canonical location cannot be created or opened, falls back to `./icetea.log`.
    pub fn open_log_file(explicit: Option<&Path>) -> Result<(std::fs::File, PathBuf)> {
        use std::fs::OpenOptions;

        if let Some(path) = explicit {
            if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
                std::fs::create_dir_all(parent).with_context(|| {
                    format!("Failed to create log directory {}", parent.display())
                })?;
            }
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .with_context(|| format!("Failed to open log file {}", path.display()))?;
            return Ok((file, path.to_path_buf()));
        }

        let path = Self::default_log_path();
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty())
            && let Err(err) = std::fs::create_dir_all(parent) {
                eprintln!(
                    "warning: could not create log directory {}: {err}; falling back to ./icetea.log",
                    parent.display()
                );
                return open_cwd_log();
            }

        match OpenOptions::new().create(true).append(true).open(&path) {
            Ok(file) => Ok((file, path)),
            Err(err) => {
                eprintln!(
                    "warning: could not open log file {}: {err}; falling back to ./icetea.log",
                    path.display()
                );
                open_cwd_log()
            }
        }
    }

    fn default_config() -> Self {
        Self {
            catalogs: HashMap::new(),
            ui: UiConfig::default(),
            query: QueryConfig::default(),
        }
    }
}

fn resolve_log_path(
    xdg_state_home: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
) -> PathBuf {
    let state_home = xdg_state_home
        .map(PathBuf::from)
        .or_else(|| home.map(|h| PathBuf::from(h).join(".local/state")));

    match state_home {
        Some(dir) => dir.join("icetea").join("icetea.log"),
        None => PathBuf::from("icetea.log"),
    }
}

fn open_cwd_log() -> Result<(std::fs::File, PathBuf)> {
    let path = PathBuf::from("icetea.log");
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .context("Failed to open fallback log file ./icetea.log")?;
    Ok((file, path))
}

/// Parse catalog URI specs: `name=type:uri` or bare `uri` (defaults to rest / catalog_N)
fn parse_catalog_uris(uris: &[String]) -> Result<HashMap<String, CatalogConfig>> {
    let mut catalogs = HashMap::new();

    for (idx, uri) in uris.iter().enumerate() {
        let (name, catalog_type, uri) = if let Some((name, rest)) = uri.split_once('=') {
            if let Some((catalog_type, uri)) = rest.split_once(':') {
                (name.to_string(), catalog_type.to_string(), uri.to_string())
            } else {
                (name.to_string(), "rest".to_string(), rest.to_string())
            }
        } else {
            (
                format!("catalog_{}", idx),
                "rest".to_string(),
                uri.clone(),
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

/// Parse `name=value` pairs used by warehouse flags
fn parse_name_value(spec: &str, flag: &str) -> Result<(String, String)> {
    let Some((name, value)) = spec.split_once('=') else {
        bail!("Invalid --{flag} `{spec}`: expected name=value");
    };
    if name.is_empty() || value.is_empty() {
        bail!("Invalid --{flag} `{spec}`: name and value must be non-empty");
    }
    Ok((name.to_string(), value.to_string()))
}

/// Parse `name.key=value` property specs (key may contain dots, e.g. `s3.region`)
fn parse_catalog_property(spec: &str) -> Result<(String, String, String)> {
    let Some((name_key, value)) = spec.split_once('=') else {
        bail!("Invalid --catalog-property `{spec}`: expected name.key=value");
    };
    let Some((name, key)) = name_key.split_once('.') else {
        bail!("Invalid --catalog-property `{spec}`: expected name.key=value");
    };
    if name.is_empty() || key.is_empty() || value.is_empty() {
        bail!("Invalid --catalog-property `{spec}`: name, key, and value must be non-empty");
    }
    Ok((name.to_string(), key.to_string(), value.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_cli() -> Cli {
        Cli {
            config: None,
            log_file: None,
            verbose: false,
            catalogs: vec![],
            catalog_warehouses: vec![],
            catalog_properties: vec![],
            theme: None,
            refresh_interval: None,
            query_timeout: None,
            max_rows: None,
            command: None,
        }
    }

    #[test]
    fn resolve_log_path_prefers_xdg_state_home() {
        let path = resolve_log_path(
            Some("/tmp/xdg-state-test".into()),
            Some("/home/user".into()),
        );
        assert_eq!(path, PathBuf::from("/tmp/xdg-state-test/icetea/icetea.log"));
    }

    #[test]
    fn resolve_log_path_falls_back_to_home_local_state() {
        let path = resolve_log_path(None, Some("/home/user".into()));
        assert_eq!(
            path,
            PathBuf::from("/home/user/.local/state/icetea/icetea.log")
        );
    }

    #[test]
    fn resolve_log_path_falls_back_to_cwd_without_home() {
        let path = resolve_log_path(None, None);
        assert_eq!(path, PathBuf::from("icetea.log"));
    }

    #[test]
    fn parse_catalog_uris_named_with_type() {
        let catalogs =
            parse_catalog_uris(&["local=rest:http://localhost:8181".to_string()]).unwrap();
        let cat = catalogs.get("local").unwrap();
        assert_eq!(cat.catalog_type, "rest");
        assert_eq!(cat.uri, "http://localhost:8181");
        assert!(cat.warehouse.is_none());
    }

    #[test]
    fn parse_catalog_uris_bare_uri() {
        let catalogs = parse_catalog_uris(&["http://localhost:8181".to_string()]).unwrap();
        let cat = catalogs.get("catalog_0").unwrap();
        assert_eq!(cat.catalog_type, "rest");
        assert_eq!(cat.uri, "http://localhost:8181");
    }

    #[test]
    fn parse_catalog_property_with_dotted_key() {
        let (name, key, value) =
            parse_catalog_property("prod.s3.access-key-id=AKIA123").unwrap();
        assert_eq!(name, "prod");
        assert_eq!(key, "s3.access-key-id");
        assert_eq!(value, "AKIA123");
    }

    #[test]
    fn parse_catalog_property_rejects_bad_format() {
        assert!(parse_catalog_property("noequals").is_err());
        assert!(parse_catalog_property("nodot=value").is_err());
        assert!(parse_catalog_property(".key=value").is_err());
    }

    #[test]
    fn cli_overrides_beat_defaults() {
        let mut config = Config::default_config();
        let mut cli = empty_cli();
        cli.theme = Some("light".to_string());
        cli.refresh_interval = Some(60);
        cli.query_timeout = Some(120);
        cli.max_rows = Some(500);
        cli.catalogs = vec!["demo=rest:http://example:8181".to_string()];
        cli.catalog_warehouses = vec!["demo=s3://wh".to_string()];
        cli.catalog_properties = vec![
            "demo.token=abc".to_string(),
            "demo.s3.region=us-east-1".to_string(),
        ];

        config.apply_cli_overrides(&cli).unwrap();

        assert_eq!(config.ui.theme, "light");
        assert_eq!(config.ui.refresh_interval, 60);
        assert_eq!(config.query.timeout, 120);
        assert_eq!(config.query.max_rows, 500);

        let cat = config.catalogs.get("demo").unwrap();
        assert_eq!(cat.uri, "http://example:8181");
        assert_eq!(cat.warehouse.as_deref(), Some("s3://wh"));
        assert_eq!(cat.properties.get("token").map(String::as_str), Some("abc"));
        assert_eq!(
            cat.properties.get("s3.region").map(String::as_str),
            Some("us-east-1")
        );
    }

    #[test]
    fn cli_catalog_updates_existing_without_wiping_properties() {
        let mut config = Config::default_config();
        config.catalogs.insert(
            "demo".to_string(),
            CatalogConfig {
                catalog_type: "rest".to_string(),
                uri: "http://old:8181".to_string(),
                warehouse: Some("s3://old".to_string()),
                properties: HashMap::from([("token".to_string(), "keep-me".to_string())]),
            },
        );

        let mut cli = empty_cli();
        cli.catalogs = vec!["demo=rest:http://new:8181".to_string()];
        config.apply_cli_overrides(&cli).unwrap();

        let cat = config.catalogs.get("demo").unwrap();
        assert_eq!(cat.uri, "http://new:8181");
        assert_eq!(cat.warehouse.as_deref(), Some("s3://old"));
        assert_eq!(
            cat.properties.get("token").map(String::as_str),
            Some("keep-me")
        );
    }

    #[test]
    fn warehouse_for_unknown_catalog_errors() {
        let mut config = Config::default_config();
        let mut cli = empty_cli();
        cli.catalog_warehouses = vec!["missing=s3://wh".to_string()];
        assert!(config.apply_cli_overrides(&cli).is_err());
    }
}
